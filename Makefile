SHELL := /bin/bash
PKG_NAME = sand-timer
DESTDIR ?=
PREFIX ?= /usr/local
BINDIR = $(DESTDIR)$(PREFIX)/bin
SYSTEMD_USER_DIR = $(DESTDIR)$(PREFIX)/lib/systemd/user
BINARY ?= target/release/sand

.PHONY: all
all: sand

.PHONY: sand
sand:
	cargo build --release

.PHONY: sand-debug
sand-debug:
	cargo build


.PHONY: install
install:
	@echo "NOTE: we don't depend on the sand or sand-debug targets for install."
	@echo "Therefore, remember to run make first, to make sure you're installing"
	@echo "the correct binary."
	@echo
	@echo "The reason is to avoid accidentally building as root, causing permission"
	@echo "issues and causing cargo to subsequently rebuild unnecessarily"
	@echo

	install -Dm755 "$(BINARY)" "$(BINDIR)/sand"
	install -Dm644 README.md "$(DESTDIR)$(PREFIX)/share/doc/$(PKG_NAME)/README.md"
	install -Dm644 LICENSE "$(DESTDIR)$(PREFIX)/share/licenses/$(PKG_NAME)/LICENSE"

	install -Dm644 resources/systemd/sand.socket "$(SYSTEMD_USER_DIR)/sand.socket"
	install -Dm644 \
	    <(sed "s|@prefix@|$(PREFIX)|" resources/systemd/sand.service.in) \
		"$(SYSTEMD_USER_DIR)/sand.service"

	install -Dm644 resources/timer_sound.flac "$(DESTDIR)$(PREFIX)/share/$(PKG_NAME)/timer_sound.flac"

.PHONY: install-debug
install-debug:
	$(MAKE) DESTDIR="$(DESTDIR)" PREFIX="$(PREFIX)" BINARY=target/debug/sand install

.PHONY: uninstall
uninstall:
	rm -f "$(BINDIR)/sand"
	rm -f "$(DESTDIR)$(PREFIX)/share/doc/$(PKG_NAME)/README.md"
	rmdir "$(DESTDIR)$(PREFIX)/share/doc/$(PKG_NAME)"
	rm -f "$(DESTDIR)$(PREFIX)/share/licenses/$(PKG_NAME)/LICENSE"
	rmdir "$(DESTDIR)$(PREFIX)/share/licenses/$(PKG_NAME)"
	rm -f "$(DESTDIR)$(PREFIX)/lib/systemd/user/sand.socket"
	rm -f "$(DESTDIR)$(PREFIX)/lib/systemd/user/sand.service"
	rm -f "$(DESTDIR)$(PREFIX)/share/$(PKG_NAME)/timer_sound.flac"
	rmdir "$(DESTDIR)$(PREFIX)/share/$(PKG_NAME)"
