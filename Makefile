WEECHAT_HOME ?= $(HOME)/.weechat
PREFIX ?= $(WEECHAT_HOME)
SOURCES = src/lib.rs src/buffer.rs


install: target/debug/libripgrep.so install-dir
	install -m644 target/debug/libripgrep.so $(DESTDIR)$(PREFIX)/plugins/ripgrep.so

install-dir:
	install -d $(DESTDIR)$(PREFIX)/plugins

target/debug/libripgrep.so: $(SOURCES)
	cargo build

format:
	cargo fmt

.PHONY: format install install-dir
