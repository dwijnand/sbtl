#!/usr/bin/env bats

load test_helper

setup()    { create_project; stub_java_echo; }
teardown() { unstub java; rm -fr "$TEST_ROOT"/* "$TEST_ROOT"/.sbt; }

launcher_url () {
  case "$1" in
    0.7.*) echo "http://simple-build-tool.googlecode.com/files/sbt-launch-$1.jar" ;;
   0.10.*) echo "http://repo.typesafe.com/typesafe/ivy-releases/org.scala-tools.sbt/sbt-launch/$1/sbt-launch.jar" ;;
        *) echo "http://repo.typesafe.com/typesafe/ivy-releases/org.scala-sbt/sbt-launch/$1/sbt-launch.jar" ;;
  esac
}

write_to_properties_and_launch ()         { write_to_properties "$1" && shift && launch_launcher "$@"; }
write_version_to_properties_and_launch () { write_version_to_properties "$1" && launch_launcher "$@"; }
no_properties_and_launch ()               { assert [ ! -f "$test_build_properties" ] && launch_launcher "$@"; }

launch_launcher() {
  local version="$1" && shift
  run sbt "$@"
  assert_success
  assert_output <<EOS
Downloading sbt launcher for $version:
  From  $(launcher_url $version)
    To  $TEST_ROOT/.sbt/launchers/$version/sbt-launch.jar
java
-XX:MaxPermSize=384m
-Xms512m
-Xmx1536m
-Xss2m
-jar
\$ROOT/.sbt/launchers/$version/sbt-launch.jar
shell
EOS
}

@test "launches sbt 0.7.x"  { write_version_to_properties_and_launch "$sbt_07"; }
@test "launches sbt 0.10.x" { write_version_to_properties_and_launch "$sbt_10"; }
@test "launches sbt 0.11.x" { write_version_to_properties_and_launch "$sbt_11"; }
@test "launches sbt 0.12.x" { write_version_to_properties_and_launch "$sbt_12"; }
@test "launches sbt 0.13.x" { write_version_to_properties_and_launch "$sbt_13"; }

@test "allows surrounding white spaces around '=' in build.properties" {
  write_to_properties_and_launch "sbt.version = 0.12.1" "0.12.1"
}

@test "supports windows line endings (crlf) in build.properties" {
  write_to_properties_and_launch "sbt.version=0.22.0-M1\r\n" "0.22.0-M1"
}

@test "supports unix line endings (lf) in build.properties" {
  write_to_properties_and_launch "sbt.version=0.13.14\n" "0.13.14"
}

@test "skips any irrelevant lines in build.properties" {
  write_to_properties_and_launch "# hand written:\n\nsbt.version=0.13.14\nsbt.something = else\n" "0.13.14"
}
