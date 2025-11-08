#!/usr/bin/env python3

"""
Sand integration tests.
"""

import time
import socket
import os
import subprocess
import json
import warnings
from contextlib import contextmanager
from pprint import pformat

import pytest
from deepdiff import DeepDiff

SOCKET_PATH = "./test.sock"

# determine which target to test from env
target = os.environ.get("SAND_TEST_TARGET", "debug")
if target == "release":
    BINARY_PATH = "./target/release/sand"
elif target == "debug":
    BINARY_PATH = "./target/debug/sand"
else:
    raise ValueError(f"Unknown target: {target}")


def log(s):
    t = time.strftime("%H:%M:%S")
    print(f"Tests [{t}] {s}")


def ensure_deleted(path):
    """
    Remove the socket file if it already exists
    """
    try:
        os.unlink(path)
    except FileNotFoundError:
        pass


# TODO refactor daemon tests to use fake client, client tests to use fake daemon
# they should be testable independently


@pytest.fixture
def daemon():
    ensure_deleted(SOCKET_PATH)
    daemon_args = ["daemon"]
    try:
        with open("daemon_stderr.log", "w") as daemon_stderr:
            daemon_proc = subprocess.Popen(
                [BINARY_PATH] + daemon_args,
                env={
                    "SAND_SOCK_PATH": SOCKET_PATH,
                    "RUST_LOG": "trace",
                },
                stderr=daemon_stderr,
            )
            wait_for_socket(SOCKET_PATH)
            log(f"Daemon started with PID {daemon_proc.pid}")
            yield daemon_proc
    finally:
        log(f"Terminating daemon with PID {daemon_proc.pid}")
        daemon_proc.terminate()
        log("Waiting for daemon to terminate")
        daemon_proc.wait()
        log("Daemon terminated")


def wait_for_socket(path, timeout=5):
    start = time.time()
    while not os.path.exists(path):
        elapsed = time.time() - start
        if elapsed > timeout:
            raise TimeoutError(f"Socket {path} not created within {timeout}s")
        time.sleep(0.001)


def run_client(sock_path, args):
    client_proc = subprocess.Popen(
        [BINARY_PATH] + args,
        env={"SAND_SOCK_PATH": sock_path},
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    status = client_proc.wait()
    stdout = client_proc.stdout.read().decode("utf-8")
    stderr = client_proc.stderr.read().decode("utf-8")
    if status != 0:
        log(f"Client exited with status {status}")
        log(f"Client stderr:\n{stderr}")
    return {"status": status, "stdout": stdout, "stderr": stderr}


class TestClient:
    def test_list_none(self, daemon):
        result = run_client(SOCKET_PATH, ["list"])
        assert result["status"] == 0, f"Client exited with status {result['status']}"
        expected_stdout = "There are currently no timers."
        assert result["stdout"].strip() == expected_stdout

    def test_add(self, daemon):
        result = run_client(SOCKET_PATH, ["start", "10m"])
        assert result["status"] == 0, f"Client exited with status {result['status']}"
        expected_stdout = "Timer #1 created for 00:10:00.000."
        assert result["stdout"].strip() == expected_stdout


@contextmanager
def client_socket():
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client_sock:
        client_sock.connect(SOCKET_PATH)
        yield client_sock


def msg_and_response(msg):
    msg_bytes = bytes(json.dumps(msg) + "\n", encoding="utf-8")
    with client_socket() as sock:
        sock.sendall(msg_bytes)
        resp_bytes = sock.recv(1024)
    response = json.loads(resp_bytes.decode("utf-8"))
    return response


# Since the amount of time elapsed is not deterministic, for most tests we want
# to ignore the specific amount of time elapsed/remaining.
IGNORE_MILLIS = r".+\['millis'\]$"
IGNORE_REMAINING = r".+\['remaining'\]$"


class TestDaemon:
    def test_list_none(self, daemon):
        response = msg_and_response("list")

        expected_shape = {"ok": {"timers": []}}

        diff = DeepDiff(
            expected_shape,
            response,
            exclude_regex_paths=IGNORE_MILLIS,
            ignore_order=True,
        )
        assert not diff, f"Response shape mismatch:\n{pformat(diff)}"

    def test_add(self, daemon):
        msg = {"starttimer": {"duration": {"secs": 60, "nanos": 0}}}
        expected = {"ok": {"id": 1}}

        response = msg_and_response(msg)
        diff = DeepDiff(expected, response, ignore_order=True)
        assert not diff, f"Response shape mismatch:\n{pformat(diff)}"

    def test_add_with_message(self, daemon):
        msg_and_response(
            {
                "starttimer": {
                    "duration": {"secs": 10 * 60, "nanos": 0},
                    "message": "Hello, world!",
                }
            }
        )
        response = msg_and_response("list")
        expected_shape = {
            "ok": {
                "timers": [
                    {
                        "id": 1,
                        "message": "Hello, world!",
                        "state": "Running",
                        "remaining": None,
                    },
                ]
            }
        }
        diff = DeepDiff(
            expected_shape,
            response,
            exclude_regex_paths=IGNORE_REMAINING,
            ignore_order=True,
        )
        assert not diff, f"Response shape mismatch:\n{pformat(diff)}"

    def test_list(self, daemon):
        msg_and_response({"starttimer": {"duration": {"secs": 10 * 60, "nanos": 0}}})
        msg_and_response({"starttimer": {"duration": {"secs": 20 * 60, "nanos": 0}}})

        response = msg_and_response("list")

        expected_shape = {
            "ok": {
                "timers": [
                    {"id": 2, "message": None, "state": "Running", "remaining": None},
                    {"id": 1, "message": None, "state": "Running", "remaining": None},
                ]
            }
        }

        diff = DeepDiff(
            expected_shape,
            response,
            exclude_regex_paths=IGNORE_REMAINING,
            ignore_order=True,
        )
        assert not diff, f"Response shape mismatch:\n{pformat(diff)}"

    def test_pause_resume(self, daemon):
        run_client(SOCKET_PATH, ["start", "10m"])
        run_client(SOCKET_PATH, ["pause", "1"])

        response = msg_and_response("list")
        expected_shape = {
            "ok": {
                "timers": [
                    {"id": 1, "message": None, "state": "Paused", "remaining": None}
                ]
            }
        }
        diff = DeepDiff(
            expected_shape,
            response,
            exclude_regex_paths=IGNORE_REMAINING,
            ignore_order=True,
        )
        assert not diff, f"Response shape mismatch:\n{pformat(diff)}"

        run_client(SOCKET_PATH, ["resume", "1"])

        response = msg_and_response("list")
        expected_shape = {
            "ok": {
                "timers": [
                    {"id": 1, "message": None, "state": "Running", "remaining": None}
                ]
            }
        }
        diff = DeepDiff(
            expected_shape,
            response,
            exclude_regex_paths=IGNORE_REMAINING,
            ignore_order=True,
        )
        assert not diff, f"Response shape mismatch:\n{pformat(diff)}"

    def test_cancel(self, daemon):
        run_client(SOCKET_PATH, ["start", "10m"])
        run_client(SOCKET_PATH, ["cancel", "1"])

        response = msg_and_response("list")
        expected_shape = {"ok": {"timers": []}}
        diff = DeepDiff(expected_shape, response, ignore_order=True)
        assert not diff, f"Response shape mismatch:\n{pformat(diff)}"

    def test_cancel_paused(self, daemon):
        run_client(SOCKET_PATH, ["start", "10m"])
        run_client(SOCKET_PATH, ["pause", "1"])

        response = msg_and_response("list")
        expected_shape = {
            "ok": {
                "timers": [
                    {"id": 1, "message": None, "state": "Paused", "remaining": None}
                ]
            }
        }
        diff = DeepDiff(
            expected_shape,
            response,
            exclude_regex_paths=IGNORE_REMAINING,
            ignore_order=True,
        )
        assert not diff, f"Response shape mismatch:\n{pformat(diff)}"

        run_client(SOCKET_PATH, ["cancel", "1"])

        response = msg_and_response("list")
        expected_shape = {"ok": {"timers": []}}
        diff = DeepDiff(expected_shape, response, ignore_order=True)
        assert not diff, f"Response shape mismatch:\n{pformat(diff)}"

    def test_again(self, daemon):
        response = msg_and_response("again")
        expected = "nonepreviouslystarted"
        assert response == expected

        run_client(SOCKET_PATH, ["start", "1s"])
        response = msg_and_response("again")
        expected_shape = {"ok": {"id": 2, "duration": 1000}}
        diff = DeepDiff(expected_shape, response, ignore_order=True)
        assert not diff, f"Response shape mismatch:\n{pformat(diff)}"


"""
Need to get this down. I think by eliminating any `import Lean`.
Hopefully we'll be able to make the warn_threshold the fail_threshold
"""


@pytest.mark.skipif(
    target == "debug", reason="Only check executable size in release builds"
)
def test_executable_size():
    exe_size = os.path.getsize(BINARY_PATH)
    warn_threshold = 8_000_000
    if exe_size > warn_threshold:
        exe_size_mb = exe_size / 1_000_000
        warnings.warn(f"Sand executable size is {exe_size_mb:.2f}MB")

    fail_threshold = 10_000_000
    assert exe_size < fail_threshold, f"Sand executable size is {exe_size_mb:.2f}MB"


if __name__ == "__main__":
    pytest.main([__file__])
