WEECHAT_HOME ?= $(HOME)/.weechat
PREFIX ?= $(WEECHAT_HOME)
SOURCES = src/lib.rs src/buffer.rs


install: target/release/libripgrep.so install-dir
	install -m644 target/release/libripgrep.so $(DESTDIR)$(PREFIX)/plugins/ripgrep.so

install-dir:
	install -d $(DESTDIR)$(PREFIX)/plugins

target/release/libripgrep.so: $(SOURCES)
	cargo build --release

format:
	cargo fmt

.PHONY: format install install-dir
