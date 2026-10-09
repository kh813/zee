.PHONY: all default local tui cli gui test check install package clean help

SHELL := /bin/bash

# Detect OS and Architecture
UNAME_S := $(shell uname -s 2>/dev/null || echo Windows_NT)
UNAME_M := $(shell uname -m 2>/dev/null || echo x86_64)

ifeq ($(UNAME_S),Darwin)
    OS_TYPE := macos
    EXE_EXT :=
    GUI_TARGET := macos-gui
    ZEE_GUI_BIN := zeeg
    ZEE_TUI_BIN := zee
else ifeq ($(findstring MINGW,$(UNAME_S)),MINGW)
    OS_TYPE := windows
    EXE_EXT := .exe
    GUI_TARGET := windows-gui
    ZEE_GUI_BIN := zee.exe
    ZEE_TUI_BIN := zee-tui.exe
else ifeq ($(findstring MSYS,$(UNAME_S)),MSYS)
    OS_TYPE := windows
    EXE_EXT := .exe
    GUI_TARGET := windows-gui
    ZEE_GUI_BIN := zee.exe
    ZEE_TUI_BIN := zee-tui.exe
else ifeq ($(UNAME_S),Windows_NT)
    OS_TYPE := windows
    EXE_EXT := .exe
    GUI_TARGET := windows-gui
    ZEE_GUI_BIN := zee.exe
    ZEE_TUI_BIN := zee-tui.exe
else
    OS_TYPE := linux
    EXE_EXT :=
    GUI_TARGET := linux-gui
    ZEE_GUI_BIN := zeeg
    ZEE_TUI_BIN := zee
endif

DIST_DIR := dist

# Default: build for the current OS
default: local

ifeq ($(OS_TYPE),windows)
local: gui
	@echo ""
	@echo "==> Build complete for $(OS_TYPE) ($(UNAME_M)) in $(DIST_DIR)/"
else
local: tui gui
	@echo ""
	@echo "==> Build complete for $(OS_TYPE) ($(UNAME_M)) in $(DIST_DIR)/"
endif

all: local

tui: cli
cli:
	@mkdir -p $(DIST_DIR)
	@echo "==> Building TUI (zee)..."
	cargo build --release -p zee-tui
	@cp target/release/$(ZEE_TUI_BIN) $(DIST_DIR)/$(ZEE_TUI_BIN)
	@echo "Built $(DIST_DIR)/$(ZEE_TUI_BIN)"

gui: $(GUI_TARGET)

macos-gui:
	@mkdir -p $(DIST_DIR)
	@echo "==> Building macOS GUI (Zee.app)..."
	cargo build --release -p zee-gui $(CARGO_FLAGS)
	@rm -rf $(DIST_DIR)/Zee.app $(DIST_DIR)/led.app
	@mkdir -p $(DIST_DIR)/Zee.app/Contents/MacOS
	@mkdir -p $(DIST_DIR)/Zee.app/Contents/Resources
	@BIN_PATH=$$(find target -name zeeg -type f | grep release | head -n 1); \
	if [ -z "$$BIN_PATH" ]; then BIN_PATH="target/release/zeeg"; fi; \
	cp "$$BIN_PATH" $(DIST_DIR)/Zee.app/Contents/MacOS/zeeg; \
	chmod +x $(DIST_DIR)/Zee.app/Contents/MacOS/zeeg
	@if [ -f assets/icons/zee.icns ]; then cp assets/icons/zee.icns $(DIST_DIR)/Zee.app/Contents/Resources/zee.icns; fi
	@echo '<?xml version="1.0" encoding="UTF-8"?>' > $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '<plist version="1.0">' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '<dict>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '    <key>CFBundleExecutable</key>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '    <string>zeeg</string>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '    <key>CFBundleIdentifier</key>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '    <string>dev.hiroshi.zeeg</string>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '    <key>CFBundleName</key>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '    <string>zee</string>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '    <key>CFBundleDisplayName</key>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '    <string>zee</string>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '    <key>CFBundlePackageType</key>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '    <string>APPL</string>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '    <key>CFBundleShortVersionString</key>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '    <string>0.1.7</string>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '    <key>CFBundleVersion</key>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '    <string>0.1.7</string>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '    <key>CFBundleIconFile</key>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '    <string>zee.icns</string>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '    <key>NSHighResolutionCapable</key>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '    <true/>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '    <key>NSSupportsAutomaticGraphicsSwitching</key>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '    <true/>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '    <key>CFBundleDocumentTypes</key>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '    <array>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '        <dict>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '            <key>CFBundleTypeName</key>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '            <string>All Files</string>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '            <key>CFBundleTypeRole</key>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '            <string>Editor</string>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '            <key>LSItemContentTypes</key>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '            <array>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '                <string>public.data</string>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '                <string>public.content</string>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '                <string>public.plain-text</string>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '            </array>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '        </dict>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '    </array>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '    <key>LSMinimumSystemVersion</key>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '    <string>10.15.7</string>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '</dict>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@echo '</plist>' >> $(DIST_DIR)/Zee.app/Contents/Info.plist
	@touch $(DIST_DIR)/Zee.app
	@echo "Built $(DIST_DIR)/Zee.app"

linux-gui:
	@mkdir -p $(DIST_DIR)
	@echo "==> Building Linux GUI (zeeg)..."
	cargo build --release -p zee-gui $(CARGO_FLAGS)
	@cp target/release/zeeg $(DIST_DIR)/zeeg
	@echo "Built $(DIST_DIR)/zeeg"

windows-gui:
	@mkdir -p $(DIST_DIR)
	@echo "==> Building Windows GUI (zee.exe)..."
	cargo build --release -p zee-gui $(CARGO_FLAGS)
	@cp target/release/zeeg.exe $(DIST_DIR)/zee.exe
	@echo "Built $(DIST_DIR)/zee.exe"

test:
	@echo "==> Running workspace tests..."
	cargo test --workspace

check:
	@echo "==> Checking workspace..."
	cargo check --workspace --all-targets

install: local
	@echo "==> Installing binaries..."
	@INSTALL_DIR=$${HOME}/.local/bin; \
	mkdir -p $$INSTALL_DIR; \
	if [ "$(OS_TYPE)" != "windows" ]; then \
		cp $(DIST_DIR)/$(ZEE_TUI_BIN) $$INSTALL_DIR/; \
		echo "Installed $(ZEE_TUI_BIN) to $$INSTALL_DIR/"; \
	fi; \
	if [ -f $(DIST_DIR)/$(ZEE_GUI_BIN) ]; then \
		cp $(DIST_DIR)/$(ZEE_GUI_BIN) $$INSTALL_DIR/; \
		echo "Installed $(ZEE_GUI_BIN) to $$INSTALL_DIR/"; \
	fi; \
	if [ "$(OS_TYPE)" = "linux" ]; then \
		mkdir -p $${HOME}/.local/share/applications; \
		if [ -f assets/zeeg.desktop ]; then \
			cp assets/zeeg.desktop $${HOME}/.local/share/applications/; \
			echo "Installed zeeg.desktop to $${HOME}/.local/share/applications/"; \
		fi; \
		mkdir -p $${HOME}/.local/share/icons/hicolor/scalable/apps; \
		if [ -f assets/icons/zee.svg ]; then \
			cp assets/icons/zee.svg $${HOME}/.local/share/icons/hicolor/scalable/apps/zee.svg; \
			echo "Installed icon to $${HOME}/.local/share/icons/hicolor/scalable/apps/"; \
		fi; \
	fi; \
	if [ "$(OS_TYPE)" = "macos" ] && [ -d $(DIST_DIR)/Zee.app ]; then \
		mkdir -p $${HOME}/Applications; \
		rm -rf $${HOME}/Applications/Zee.app; \
		cp -r $(DIST_DIR)/Zee.app $${HOME}/Applications/; \
		echo "Installed Zee.app to $${HOME}/Applications/"; \
	fi

package: local
	@echo "==> Packaging release archives..."
	@cd $(DIST_DIR) && \
	if [ "$(OS_TYPE)" = "macos" ]; then \
		tar -czvf zee-$(OS_TYPE)-$(UNAME_M).tar.gz $(ZEE_TUI_BIN); \
		if [ -d Zee.app ]; then zip -r zeeg-$(OS_TYPE)-$(UNAME_M).zip Zee.app; fi; \
	elif [ "$(OS_TYPE)" = "windows" ]; then \
		zip -r zeeg-$(OS_TYPE)-$(UNAME_M).zip $(ZEE_GUI_BIN); \
	else \
		tar -czvf zee-$(OS_TYPE)-$(UNAME_M).tar.gz $(ZEE_TUI_BIN); \
		if [ -f $(ZEE_GUI_BIN) ]; then tar -czvf zeeg-$(OS_TYPE)-$(UNAME_M).tar.gz $(ZEE_GUI_BIN); fi; \
	fi
	@echo "Created package archives in $(DIST_DIR)/"

clean:
	rm -rf $(DIST_DIR)
	cargo clean

help:
	@echo "zee build targets:"
	@echo "  make              - Build for host OS into $(DIST_DIR)/"
	@echo "  make tui          - Build TUI binary (zee) into $(DIST_DIR)/"
	@echo "  make gui          - Build GUI binary (and Zee.app on macOS) into $(DIST_DIR)/"
	@echo "  make test         - Run workspace unit & integration tests"
	@echo "  make check        - Run cargo check across all crates"
	@echo "  make install      - Install binaries to ~/.local/bin (and ~/Applications on macOS)"
	@echo "  make package      - Create release archive (tar.gz / zip) in $(DIST_DIR)/"
	@echo "  make clean        - Remove $(DIST_DIR)/ and target/ build artifacts"
	@echo "  make help         - Show this help message"
