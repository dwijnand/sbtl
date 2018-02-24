#![allow(dead_code)]
#![allow(non_upper_case_globals)]
//#![allow(unused_assignments)]
#![allow(unused_imports)]
//#![allow(unused_variables)]

use std::env;
use std::ffi::OsStr;
use std::fmt::Display;
use std::fs;
use std::fs::File;
use std::io::{ BufReader, BufWriter, };
use std::io::prelude::*;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{ Path, PathBuf, };
use std::process::{ Command, exit, };

use jsonrpc_lite;
use jsonrpc_lite::JsonRpc;

use serde_json;
use serde_json::Value;

fn make_lsp_json_str(method: &str, params: Value) -> Result<String, serde_json::error::Error> {
    let msg = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });
    let request = serde_json::to_string(&msg)?;
    Ok(format!("Content-Length: {}\r\n\r\n{}", request.len(), request))
}

#[derive(Debug, PartialEq)]
/// A message header, as described in the Language Server Protocol specification.
enum LspHeader {
    ContentType,
    ContentLength(usize),
}

/// Given a reference to a reader, attempts to read a Language Server Protocol message,
/// blocking until a message is received.
fn read_message<B: BufRead>(reader: &mut B) -> Value {
    let mut buffer = String::new();
    let mut content_length: Option<usize> = None;

    // read in headers.
    loop {
            buffer.clear();
            reader.read_line(&mut buffer).unwrap();
            match &buffer {
                s if s.trim().len() == 0 => { break }, // empty line is end of headers
                s => {
                    match parse_header(s) {
                        LspHeader::ContentLength(len) => content_length = Some(len),
                        LspHeader::ContentType => (), // utf-8 only currently allowed value
                    };
                }
            };
        }

    let content_length = content_length.ok_or(format!("missing content-length header: {}", buffer)).unwrap();
    // message body isn't newline terminated, so we read content_length bytes
    let mut body_buffer = vec![0; content_length];
    reader.read_exact(&mut body_buffer).unwrap();
    let body = String::from_utf8(body_buffer).unwrap();
    serde_json::from_str::<Value>(&body).unwrap()
}

const HEADER_CONTENT_LENGTH: &'static str = "content-length";
const HEADER_CONTENT_TYPE: &'static str = "content-type";

/// Given a header string, attempts to extract and validate the name and value parts.
fn parse_header(s: &str) -> LspHeader {
    let split: Vec<String> = s.split(": ").map(|s| s.trim().to_lowercase()).collect();
    if split.len() != 2 { panic!(format!("malformed header: {}", s)) }
    match split[0].as_ref() {
        HEADER_CONTENT_TYPE   => LspHeader::ContentType,
        HEADER_CONTENT_LENGTH => LspHeader::ContentLength(usize::from_str_radix(&split[1], 10).unwrap()),
        _ => panic!(format!("Unknown header: {}", s)),
    }
}

fn handle_msg_quietly<B: BufRead>(mut reader: B) {
    match serde_json::from_value(read_message(&mut reader)).unwrap() {
        JsonRpc::Request(obj)    => eprintln!("client received unexpected request: {:?}", obj),
        JsonRpc::Notification(_) => (),
        JsonRpc::Success(_)      => (),
        JsonRpc::Error(obj)      => println!("recv error: {:?}", obj),
    }
}


fn handle_msg<B: BufRead>(mut reader: B) {
    match serde_json::from_value(read_message(&mut reader)).unwrap() {
        JsonRpc::Request(obj)      => eprintln!("client received unexpected request: {:?}", obj),
        JsonRpc::Notification(obj) => println!("recv notification: {:?}", obj),
        JsonRpc::Success(obj)      => println!("recv success: {:?}", obj),
        JsonRpc::Error(obj)        => println!("recv error: {:?}", obj),
    }
}

enum ExitCode { Success, Failure }

fn handle_msg_to_exit_code<B: BufRead>(mut reader: B) -> ExitCode {
    let mut done = false;
    let mut success = false;
    let mut failure = false;

    loop {
        let json_rpc = serde_json::from_value(read_message(&mut reader)).unwrap();
        match json_rpc {
            JsonRpc::Request(obj)          => eprintln!("client received unexpected request: {:?}", obj),
            JsonRpc::Success(obj)          => println!("recv success: {:?}", obj),
            JsonRpc::Error(obj)            => println!("recv error: {:?}", obj),
            JsonRpc::Notification(ref obj) => {
                match json_rpc.get_method() {
                    Some("window/logMessage") => {
                        let params0 = json_rpc.get_params().unwrap();
                        let params = match params0 {
                            jsonrpc_lite::Params::Array(_) => panic!("not expecting array"),
                            jsonrpc_lite::Params::None(()) => panic!("not expecting none"),
                            jsonrpc_lite::Params::Map(kvs) => kvs,
                        };
                        let lvl = params["type"].as_i64().unwrap();
                        let msg = params["message"].as_str().unwrap();
                        println!("{}", msg);
                        if msg == "Exited with code 0" { success = true }
                        if msg == "Done" { done = true }
                        if lvl == 1 { failure = true }
                    },
                    Some("textDocument/publishDiagnostics") => {
                        let params0 = json_rpc.get_params().unwrap();
                        let params = match params0 {
                            jsonrpc_lite::Params::Array(_) => panic!("not expecting array"),
                            jsonrpc_lite::Params::None(()) => panic!("not expecting none"),
                            jsonrpc_lite::Params::Map(kvs) => kvs,
                        };
                        let uri = params["uri"].as_str().unwrap();
                        let diagnostics = params["diagnostics"].as_array().unwrap();
                        for diagnostic in diagnostics {
                            println!("{}: {}", uri, diagnostic);
                        }
                    },
                    Some(_) | _ => println!("recv notification: {:?}", obj),
                }
            },
        }

        // val Error = 1L
        // val Warning = 2L
        // val Info = 3L
        // val Log = 4L

        // 'runMain t.Main'     reports "[log] Exited with code 0" and then "[log] Done"
        // 'runMain t.BadMain'  reports "[log] Done"               and then "[error] Nonzero exit code: 1"
        // 'compile' w/ error   reports "[error] Compilation failed"
        // 'compile' w/o errors reports "[log] Done", but we can't distinguish that from BadMain's "[log] Done"...

        if success && done { return ExitCode::Success }
        if failure { return ExitCode::Failure }
    }
}

pub fn talk_to_client(port_file: File) {
    let json: serde_json::Value = serde_json::de::from_reader(port_file).unwrap();
    let uri = json["uri"].as_str().unwrap();
    // TODO: Use a less idiotic way to get the path from the URI
    let socket_file_path = &uri[8..];

    let mut stream = UnixStream::connect(socket_file_path).unwrap();
    let json_str = make_lsp_json_str("initialize", json!({})).unwrap();
    stream.write_all(json_str.as_bytes()).unwrap();
    stream.flush().unwrap();

    let mut reader = BufReader::new(stream);
    handle_msg_quietly(&mut reader);

    let args1 = env::args().skip(1); // skip the path of the executable
    let command_line = {
        // TODO: Make mk_string
        let mut s = args1.take(1).fold(String::new(), |acc, x| acc + &x + " ");
        let len = s.len() - 1;
        s.truncate(len);
        s
    };
    let json_str2 = make_lsp_json_str("sbt/exec", json!({"commandLine": command_line})).unwrap();
    let mut stream2 = UnixStream::connect(socket_file_path).unwrap();
    stream2.write_all(json_str2.as_bytes()).unwrap();
    stream2.flush().unwrap();

    let reader2 = BufReader::new(stream2);
    match handle_msg_to_exit_code(reader2) {
        ExitCode::Failure => exit(1),
        ExitCode::Success => exit(0),
    }
}
