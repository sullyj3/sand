SHELL := /bin/bash
PKG_NAME = sand-timer
DESTDIR ?=
PREFIX ?= /usr/local
BINDIR = $(DESTDIR)$(PREFIX)/bin
SYSTEMD_USER_DIR = $(DESTDIR)$(PREFIX)/lib/systemd/user

.PHONY: all
all: sand

.PHONY: sand
sand:
	cargo build --release

.PHONY: sand-debug
sand-debug:
	cargo build

.PHONY: install-bin
install-bin: sand
	install -Dm755 target/release/sand $(BINDIR)/sand

.PHONY: install-debug-bin
install-debug-bin: sand-debug
	install -Dm755 target/debug/sand $(BINDIR)/sand

.PHONY: install-rest
install-rest:
	install -Dm644 README.md $(DESTDIR)$(PREFIX)/share/doc/$(PKG_NAME)/README.md
	install -Dm644 LICENSE $(DESTDIR)$(PREFIX)/share/licenses/$(PKG_NAME)/LICENSE

	install -Dm644 resources/systemd/sand.socket $(SYSTEMD_USER_DIR)/sand.socket
	install -Dm644 \
	    <(sed "s|@prefix@|$(PREFIX)|" resources/systemd/sand.service.in) \
		$(SYSTEMD_USER_DIR)/sand.service

	install -Dm644 resources/timer_sound.flac $(DESTDIR)$(PREFIX)/share/$(PKG_NAME)/timer_sound.flac

.PHONY: install
install: install-bin install-rest

.PHONY: install-debug
install-debug: install-debug-bin install-rest

.PHONY: uninstall
uninstall:
	rm -f $(BINDIR)/sand
	rm -f $(DESTDIR)$(PREFIX)/share/doc/$(PKG_NAME)/README.md
	rmdir $(DESTDIR)$(PREFIX)/share/doc/$(PKG_NAME)
	rm -f $(DESTDIR)$(PREFIX)/share/licenses/$(PKG_NAME)/LICENSE
	rmdir $(DESTDIR)$(PREFIX)/share/licenses/$(PKG_NAME)
	rm -f $(DESTDIR)$(PREFIX)/lib/systemd/user/sand.socket
	rm -f $(DESTDIR)$(PREFIX)/lib/systemd/user/sand.service
	rm -f $(DESTDIR)$(PREFIX)/share/$(PKG_NAME)/timer_sound.flac
	rmdir $(DESTDIR)$(PREFIX)/share/$(PKG_NAME)
