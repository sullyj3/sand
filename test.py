#!/usr/bin/env python3

'''
Sand integration tests.
'''

import time
import socket
import sys
import os
import fcntl
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

'''
Remove the socket file if it already exists
'''
def ensure_deleted(path):
    try:
        os.unlink(path)
    except FileNotFoundError:
        pass

@pytest.fixture
def daemon_socket():
    ensure_deleted(SOCKET_PATH)

    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
            sock.bind(SOCKET_PATH)
            sock.listen(1)
            
            flags = fcntl.fcntl(sock.fileno(), fcntl.F_GETFD)
            flags |= fcntl.FD_CLOEXEC
            fcntl.fcntl(sock.fileno(), fcntl.F_SETFD, flags)

            log(f"Socket created at {SOCKET_PATH} on fd {sock.fileno()}")
            yield sock
    finally:
        log(f"Removing socket file {SOCKET_PATH}")
        ensure_deleted(SOCKET_PATH)

# TODO refactor daemon tests to use fake client, client tests to use fake daemon
# they should be testable independently

@pytest.fixture
def daemon(daemon_socket):

    daemon_args = ["daemon"]
    sock_fd = daemon_socket.fileno()
    try:
        with open("daemon_stderr.log", "w") as daemon_stderr:
            daemon_proc = subprocess.Popen(
                [BINARY_PATH] + daemon_args,
                pass_fds=(sock_fd,),
                env={"SAND_SOCKFD": str(sock_fd)},
                stderr=daemon_stderr,
            )

            log(f"Daemon started with PID {daemon_proc.pid}")
            # Close the socket in the parent process
            daemon_socket.close()
            yield daemon_proc
    finally:
        log(f"Terminating daemon with PID {daemon_proc.pid}")
        daemon_proc.terminate()
        log(f"Waiting for daemon to terminate")
        daemon_proc.wait()
        log(f"Daemon terminated")

def run_client(sock_path, args):
    client_proc = subprocess.Popen(
        [BINARY_PATH] + args,
        env={"SAND_SOCK_PATH": sock_path},
        stdout=subprocess.PIPE,
    )
    status = client_proc.wait()
    output = client_proc.stdout.read().decode('utf-8')
    return (status, output)

class TestClient:
    def test_list_none(self, daemon):
        (status, output) = run_client(SOCKET_PATH, ["list"])
        assert status == 0, f"Client exited with status {status}"
        expected_stdout = "No timers running."
        assert output.strip() == expected_stdout

    def test_add(self, daemon):
        (status, output) = run_client(SOCKET_PATH, ["start", "10m"])
        assert status == 0, f"Client exited with status {status}"
        expected_stdout = "Timer #1 created for 00:10:00:000."
        assert output.strip() == expected_stdout

@contextmanager
def client_socket():
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client_sock:
        client_sock.connect(SOCKET_PATH)
        yield client_sock

def msg_and_response(msg):
    msg_bytes = bytes(json.dumps(msg) + "\n", encoding='utf-8')
    with client_socket() as sock:
        sock.sendall(msg_bytes)
        resp_bytes = sock.recv(1024)
    response = json.loads(resp_bytes.decode('utf-8'))
    return response

# Since the amount of time elapsed is not deterministic, for most tests we want
# to ignore the specific amount of time elapsed/remaining.
IGNORE_MILLIS = r".+\['millis'\]$"
IGNORE_REMAINING_MILLIS = r".+\['remaining_millis'\]$"

class TestDaemon:
    def test_list_none(self, daemon):

        response = msg_and_response('list')

        expected_shape = { 'ok': { 'timers': [ ] } }
        
        diff = DeepDiff(
            expected_shape,
            response,
            exclude_regex_paths=IGNORE_MILLIS,
            ignore_order=True
        )
        assert not diff, f"Response shape mismatch:\n{pformat(diff)}"

    def test_add(self, daemon):
        msg = {'addtimer': {'duration': 60000}} 
        expected = {'ok': {'id': 1}}

        response = msg_and_response(msg)
        diff = DeepDiff(expected, response, ignore_order=True)
        assert not diff, f"Response shape mismatch:\n{pformat(diff)}"

    def test_list(self, daemon):
        msg_and_response({'addtimer': {'duration': 10 * 60 * 1000}})
        msg_and_response({'addtimer': {'duration': 20 * 60 * 1000}})
        
        response = msg_and_response('list')

        expected_shape = {
            'ok': {
                'timers': [
                    {'id': 2, 'state': 'Running', 'remaining_millis': 0},
                    {'id': 1, 'state': 'Running', 'remaining_millis': 0},
                ]
            }
        }
        
        diff = DeepDiff(
            expected_shape,
            response,
            exclude_regex_paths=IGNORE_REMAINING_MILLIS,
            ignore_order=True
        )
        assert not diff, f"Response shape mismatch:\n{pformat(diff)}"

    def test_pause_resume(self, daemon):
        run_client(SOCKET_PATH, ["start", "10m"])
        run_client(SOCKET_PATH, ["pause", "1"])

        response = msg_and_response('list')
        expected_shape = {
            'ok': {
                'timers': [
                    {
                        'id': 1,
                        'state': 'Paused', 
                        'remaining_millis': 0
                    }
                ]
            }
        }
        diff = DeepDiff(
            expected_shape,
            response,
            exclude_regex_paths=IGNORE_REMAINING_MILLIS,
            ignore_order=True
        )
        assert not diff, f"Response shape mismatch:\n{pformat(diff)}"

        run_client(SOCKET_PATH, ["resume", "1"])

        response = msg_and_response('list')
        expected_shape = {
            'ok': {
                'timers': [
                    {
                        'id': 1,
                        'state': 'Running', 
                        'remaining_millis': 0
                    }
                ]
            }
        }
        diff = DeepDiff(
            expected_shape,
            response,
            exclude_regex_paths=IGNORE_REMAINING_MILLIS,
            ignore_order=True
        )
        assert not diff, f"Response shape mismatch:\n{pformat(diff)}"

    def test_cancel(self, daemon):
        run_client(SOCKET_PATH, ["start", "10m"])
        run_client(SOCKET_PATH, ["cancel", "1"])

        response = msg_and_response('list')
        expected_shape = { 'ok': { 'timers': [] } }
        diff = DeepDiff(
            expected_shape,
            response,
            ignore_order=True
        )
        assert not diff, f"Response shape mismatch:\n{pformat(diff)}"

'''
Need to get this down. I think by eliminating any `import Lean`.
Hopefully we'll be able to make the warn_threshold the fail_threshold
'''
@pytest.mark.skipif(
    target == "debug",
    reason="Only check executable size in release builds")
def test_executable_size():
    exe_size = os.path.getsize(BINARY_PATH)
    warn_threshold = 5_000_000
    if exe_size > warn_threshold:
        exe_size_mb = exe_size / 1_000_000
        warnings.warn(f"Sand executable size is {exe_size_mb:.2f}MB")
    
    fail_threshold = 10_000_000
    assert exe_size < fail_threshold, f"Sand executable size is {exe_size_mb:.2f}MB"

if __name__ == "__main__":
    pytest.main([__file__])
