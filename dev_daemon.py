#!/usr/bin/env python3

import socket
import sys
import os
import fcntl
import subprocess
from contextlib import contextmanager

SOCKET_PATH = "./dev.sock"
BINARY_PATH = "./target/debug/sand"

'''
Remove the socket file if it already exists
'''
def ensure_deleted(path):
    try:
        os.unlink(path)
    except FileNotFoundError:
        pass

@contextmanager
def daemon_socket():
    ensure_deleted(SOCKET_PATH)

    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
            sock.bind(SOCKET_PATH)
            sock.listen(1)
            
            flags = fcntl.fcntl(sock.fileno(), fcntl.F_GETFD)
            flags |= fcntl.FD_CLOEXEC
            fcntl.fcntl(sock.fileno(), fcntl.F_SETFD, flags)

            print(f"-- Socket created at {SOCKET_PATH} on fd {sock.fileno()}")
            yield sock
    finally:
        print(f"-- Removing socket file {SOCKET_PATH}")
        ensure_deleted(SOCKET_PATH)

@contextmanager
def daemon(daemon_socket):

    daemon_args = ["daemon"]
    sock_fd = daemon_socket.fileno()
    try:
        daemon_proc = subprocess.Popen(
            [BINARY_PATH] + daemon_args,
            pass_fds=(sock_fd,),
            env={
                "SAND_SOCKFD": str(sock_fd),
                "RUST_BACKTRACE": "1",
            },
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

        print(f"-- Daemon started with PID {daemon_proc.pid}")
        # Close the socket in the parent process
        daemon_socket.close()
        yield daemon_proc
    finally:
        print(f"-- Terminating daemon with PID {daemon_proc.pid}")
        daemon_proc.terminate()
        print(f"-- Waiting for daemon to terminate")
        daemon_proc.wait()
        print(f"-- Daemon terminated")

if __name__ == "__main__":
    with daemon_socket() as sock:
        with daemon(sock) as daemon_proc:
            for l in daemon_proc.stderr:
                print(l.decode("utf-8"), end="")
            daemon_proc.wait()
