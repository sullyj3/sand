# Sand

`sand` is a linux daemon for countdown timers, and a command-line client for
interacting with it.

```console
$ sand start 5m
Timer #1 created for 00:05:00:000.

$ sand s 1h 30m
Timer #2 created for 01:30:00:000.

$ sand ls
     ID  Remaining
 ▶   #1  00:04:44:580
 ▶   #2  01:29:54:514

$ sand pause 1
Paused timer #1.

$ sand ls
     ID  Remaining
 ▶   #2  01:29:29:447

 ⏸   #1  00:04:25:017

$ sand cancel 1 2
Cancelled timer #1.
Cancelled timer #2.

$ sand ls
There are currently no timers.
```
A sound will play and a desktop notification will be triggered when a timer
elapses.

### Why not use your phone?

- I'm generally on my laptop more than my phone.
- I find it much faster and more convenient to hit super+enter to open a
  terminal and type a short command than to unlock my phone, navigate to the
  clock app, and enter the duration on the touchscreen.

### Why not use something like `sleep 600 && mpv elapsed.flac`?

Basically, a bunch of minor useability improvements.

- `sand start 10m` is less typing
- If you're anything like me, you dislike having a bunch of open windows
  cluttering your workspace, making it difficult to find things. Since sand
  runs as a daemon, you can start a timer from a terminal, immediately close
  the terminal, and the timer will still be running in the background.
  Alternatively, you can use your favorite graphical laucher or command runner.
- You get a convenient syntax for minutes and hours, so that you don't have to
  multiply by 60 or 3600 in your head to figure out how many seconds you need.
- You get the ability to see how much time is remaining. Important feature IMO!
- Sand handles suspend/sleep

If you're an ultra-minimalist who prefers cobbling solutions together using
tools that come stock with your distro, this might not be for you. If you like
convenience and good UX, this is for you.

## Installation

### Arch

You can install using the `PKGBUILD` for `sand-timer-git` in the root directory.

### Build and install from source

Sand should work on most distros. It's been tested on Arch and Ubuntu.
Please let me know if it works on your distro so that I can update this list!

1. Make sure you have the dependencies:
    - systemd
    - libnotify
    - optionally, for [audio](https://github.com/RustAudio/rodio?tab=readme-ov-file#dependencies-linux-only):
        - on Arch: alsa-lib
        - on Debian/Ubuntu: libasound2-dev
        - on Fedora: alsa-lib-devel
    - cargo, for building
2. `make`
3. `sudo make install`

## Setup
After installing, you'll need to enable and start the service.

```console
$ systemctl --user daemon-reload
$ systemctl --user enable --now sand.socket
```

To see notifications, you'll need a libnotify compatible notification server.
Desktop managers like Gnome and Plasma generally come with this built in, but
if you use a stand alone window manager you'll need to choose and install one
yourself. You can find a list of potential options [here](https://wiki.archlinux.org/title/Desktop_notifications#Standalone).

You can type
```console
$ sand start 0
```
to check everything's working correctly.
