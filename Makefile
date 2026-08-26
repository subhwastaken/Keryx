.PHONY: build run install setup clean help app install-app dev run-release

APP_NAME  = keryx
BIN       = target/release/$(APP_NAME)
DEBUG_BIN = target/debug/$(APP_NAME)
INSTALL   = /usr/local/bin

WHISPER_DIR   = $(HOME)/.config/keryx/whisper.cpp
WHISPER_BIN   = $(WHISPER_DIR)/build/bin/whisper-cli
MODEL_DIR     = $(HOME)/.config/keryx/models
MODEL_FILE    = $(MODEL_DIR)/ggml-small.en.bin
MODEL_URL     = https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin

help:
	@echo ""
	@echo "  ⚡ Keryx — High-Speed AI Voice Dictation Engine (Rust)"
	@echo ""
	@echo "  make setup    Install local whisper.cpp (free, offline, Apple Silicon GPU / CUDA)"
	@echo "  make build    Build optimized release binary"
	@echo "  make run      Build and launch (with live debug output)"
	@echo "  make app      Package native standalone Keryx.app macOS bundle"
	@echo "  make install  Install CLI binary to $(INSTALL)/$(APP_NAME)"
	@echo "  make clean    Remove build artifacts"
	@echo ""
	@echo "  Config: ~/.config/keryx/.env"
	@echo ""

# ── Setup: download and compile whisper.cpp ──────────────────────────────────
setup:
	@echo "Setting up local whisper.cpp (offline, free, cross-platform STT)..."
	@mkdir -p $(MODEL_DIR)

	@if [ ! -d "$(WHISPER_DIR)" ]; then \
		echo "Cloning whisper.cpp..."; \
		git clone --depth=1 https://github.com/ggerganov/whisper.cpp $(WHISPER_DIR); \
	else \
		echo "whisper.cpp already cloned."; \
	fi

	@echo "Building whisper.cpp for this platform..."
	@if [ "$$(uname)" = "Darwin" ]; then \
		echo "  → macOS: enabling Metal GPU (Apple Silicon/AMD)"; \
		cd $(WHISPER_DIR) && cmake -B build -DGGML_METAL=ON 2>&1 | tail -4 && cmake --build build --config Release -j4 2>&1 | tail -6; \
	elif [ "$$(uname)" = "Linux" ]; then \
		echo "  → Linux: enabling CUDA if available, else CPU"; \
		if command -v nvcc > /dev/null 2>&1; then \
			cd $(WHISPER_DIR) && cmake -B build -DGGML_CUDA=ON 2>&1 | tail -4 && cmake --build build --config Release -j4 2>&1 | tail -6; \
		else \
			cd $(WHISPER_DIR) && cmake -B build 2>&1 | tail -4 && cmake --build build --config Release -j4 2>&1 | tail -6; \
		fi; \
	else \
		echo "  → Windows/other: CPU build"; \
		cd $(WHISPER_DIR) && cmake -B build 2>&1 | tail -4 && cmake --build build --config Release -j4 2>&1 | tail -6; \
	fi

	@if [ ! -f "$(MODEL_FILE)" ]; then \
		echo "Downloading Whisper small.en model (~150MB)..."; \
		curl -L --progress-bar "$(MODEL_URL)" -o "$(MODEL_FILE)"; \
	else \
		echo "Model already downloaded."; \
	fi

	@echo ""
	@echo "[✓] whisper.cpp ready at: $$(find $(WHISPER_DIR)/build -name 'whisper-cli' -o -name 'whisper-cli.exe' 2>/dev/null | head -1)"
	@echo "[✓] Model ready at:       $(MODEL_FILE)"
	@echo ""
	@echo "Updating .env to use local STT..."
	@mkdir -p $(HOME)/.config/keryx
	@[ -f $(HOME)/.config/keryx/.env ] || cp config/.env.example $(HOME)/.config/keryx/.env 2>/dev/null || true
	@WHISPER_BIN_PATH=$$(find $(WHISPER_DIR)/build -name 'whisper-cli' -o -name 'whisper-cli.exe' 2>/dev/null | head -1); \
	sed -i '' 's|^TRANSCRIPTION_PROVIDER=.*|TRANSCRIPTION_PROVIDER=local|' $(HOME)/.config/keryx/.env 2>/dev/null || \
	sed -i  's|^TRANSCRIPTION_PROVIDER=.*|TRANSCRIPTION_PROVIDER=local|' $(HOME)/.config/keryx/.env 2>/dev/null || true; \
	sed -i '' "s|^WHISPER_BIN=.*|WHISPER_BIN=$$WHISPER_BIN_PATH|" $(HOME)/.config/keryx/.env 2>/dev/null || \
	sed -i  "s|^WHISPER_BIN=.*|WHISPER_BIN=$$WHISPER_BIN_PATH|" $(HOME)/.config/keryx/.env 2>/dev/null || true
	@sed -i '' 's|^WHISPER_MODEL=.*|WHISPER_MODEL=$(MODEL_FILE)|' $(HOME)/.config/keryx/.env 2>/dev/null || \
	 sed -i  's|^WHISPER_MODEL=.*|WHISPER_MODEL=$(MODEL_FILE)|' $(HOME)/.config/keryx/.env 2>/dev/null || true
	@echo "[✓] Config updated — restart Keryx to use local STT"


# ── Build ─────────────────────────────────────────────────────────────────────
build:
	@echo "Building Keryx (release)..."
	@cargo build --release --bin keryx
	@echo "[✓] Built: $(BIN) ($$(du -sh $(BIN) | cut -f1))"

dev:
	@cargo build --bin keryx
	@echo "[✓] Built debug: $(DEBUG_BIN)"

# ── Run ───────────────────────────────────────────────────────────────────────
run: dev
	@pkill -9 keryx 2>/dev/null || true
	@pkill -9 wisprflow 2>/dev/null || true
	@echo ""
	@echo "Starting Keryx..."
	@echo "Hold [Right Option / Right Alt] to record, release to transcribe."
	@echo "Press Esc to cancel. Ctrl+C to quit."
	@echo "─────────────────────────────────────────────────"
	@./$(DEBUG_BIN)

run-release: build
	@pkill -9 keryx 2>/dev/null || true
	@./$(BIN)

# ── Install ───────────────────────────────────────────────────────────────────
install: build
	@cp $(BIN) $(INSTALL)/$(APP_NAME)
	@chmod +x $(INSTALL)/$(APP_NAME)
	@echo "[✓] Installed to $(INSTALL)/$(APP_NAME)"

# ── Package into standalone macOS .app bundle ────────────────────────────────
app: build
	@echo "Creating Keryx.app bundle..."
	@rm -rf Keryx.app
	@mkdir -p Keryx.app/Contents/MacOS
	@mkdir -p Keryx.app/Contents/Resources
	@cp $(BIN) Keryx.app/Contents/MacOS/keryx
	@cp assets/AppIcon.icns Keryx.app/Contents/Resources/AppIcon.icns 2>/dev/null || true
	@chmod 755 Keryx.app/Contents/MacOS/keryx
	@printf "APPL????" > Keryx.app/Contents/PkgInfo
	@printf '<?xml version="1.0" encoding="UTF-8"?>\n<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">\n<plist version="1.0">\n<dict>\n    <key>CFBundleExecutable</key>\n    <string>keryx</string>\n    <key>CFBundleIdentifier</key>\n    <string>com.keryx.app</string>\n    <key>CFBundleName</key>\n    <string>Keryx</string>\n    <key>CFBundleIconFile</key>\n    <string>AppIcon</string>\n    <key>CFBundlePackageType</key>\n    <string>APPL</string>\n    <key>CFBundleShortVersionString</key>\n    <string>1.0.0</string>\n    <key>LSUIElement</key>\n    <true/>\n    <key>NSMicrophoneUsageDescription</key>\n    <string>Keryx needs microphone access for voice dictation.</string>\n    <key>NSAccessibilityUsageDescription</key>\n    <string>Keryx needs accessibility access to listen for the dictation hotkey and paste transcribed text.</string>\n</dict>\n</plist>\n' > Keryx.app/Contents/Info.plist
	@chmod -R 755 Keryx.app
	@find Keryx.app -name "._*" -delete 2>/dev/null || true
	@xattr -cr Keryx.app 2>/dev/null || true
	@codesign --force --deep --sign - --identifier "com.keryx.app" --requirements '=designated => identifier "com.keryx.app"' Keryx.app 2>/dev/null || true

# ── Install to /Applications ──────────────────────────────────────────────────
install-app: app
	@echo "Installing Keryx.app to /Applications..."
	@pkill -9 keryx 2>/dev/null || true
	@sleep 1
	@rm -f /tmp/keryx_lockfile.lock 2>/dev/null || true
	@rm -rf /Applications/Keryx.app
	@cp -R Keryx.app /Applications/Keryx.app
	@xattr -cr /Applications/Keryx.app 2>/dev/null || true
	@codesign --force --deep --sign - --identifier "com.keryx.app" --requirements '=designated => identifier "com.keryx.app"' /Applications/Keryx.app 2>/dev/null || true
	@echo "[✓] Installed to /Applications/Keryx.app"
	@echo ""
	@echo "  → Launch: open /Applications/Keryx.app"
	@echo "  → Or:     open -a Keryx"
	@echo ""
	@open /Applications/Keryx.app
	@echo "[✓] Keryx launched — look for the soundwave icon in your menu bar."

# ── Clean ─────────────────────────────────────────────────────────────────────
clean:
	@cargo clean
	@rm -rf Keryx.app WisprFlow.app
	@echo "[✓] Cleaned"
