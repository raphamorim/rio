.PHONY: docs

BUILD_MISC_DIR = misc
TARGET = rio
RELEASE_DIR = target/release

APP_NAME = Rio.app
APP_TEMPLATE = $(BUILD_MISC_DIR)/osx/$(APP_NAME)
APP_DIR = $(RELEASE_DIR)/osx
APP_BINARY = $(RELEASE_DIR)/$(TARGET)
APP_BINARY_DIR = $(APP_DIR)/$(APP_NAME)/Contents/MacOS
APP_EXTRAS_DIR = $(APP_DIR)/$(APP_NAME)/Contents/Resources
TERMINFO = $(BUILD_MISC_DIR)/rio.terminfo

all: install run

docs:
	cd docs && make run

run:
	cargo run --release

dev:
	cargo run

dev-watch:
	#cargo install cargo-watch
	cargo watch -- cargo run

install:
	cargo fetch

build: install
	cargo build --release

$(TARGET)-universal:
	MACOSX_DEPLOYMENT_TARGET="10.11" cargo build --release --target=x86_64-apple-darwin
	MACOSX_DEPLOYMENT_TARGET="10.11" cargo build --release --target=aarch64-apple-darwin
	@lipo target/{x86_64,aarch64}-apple-darwin/release/$(TARGET) -create -output $(APP_BINARY)

app-universal: $(APP_NAME)-universal ## Create a universal Rio.app
$(APP_NAME)-%: $(TARGET)-%
	@mkdir -p $(APP_BINARY_DIR)
	@mkdir -p $(APP_EXTRAS_DIR)
	@cp -fRp $(APP_TEMPLATE) $(APP_DIR)
	@cp -fp $(APP_BINARY) $(APP_BINARY_DIR)
	@tic -xe rio -o $(APP_EXTRAS_DIR) $(TERMINFO)
	@touch -r "$(APP_BINARY)" "$(APP_DIR)/$(APP_NAME)"
	@codesign --remove-signature "$(APP_DIR)/$(APP_NAME)"
	@codesign --force --deep --sign - "$(APP_DIR)/$(APP_NAME)"
	@echo "Created '$(APP_NAME)' in '$(APP_DIR)'"

release-macos: app-universal
	mkdir -p release
	cp -rf ./target/release/osx/* ./release/
	cd ./release && zip -r ./macos-rio.zip ./*

# TODO: Move to bin path
release-x11:
	cargo build --release --no-default-features --features=x11
	WINIT_UNIX_BACKEND=x11 target/release/rio
release-wayland:
	cargo build --release --no-default-features --features=wayland
	WINIT_UNIX_BACKEND=wayland target/release/rio

# cargo install cargo-wix
# https://github.com/volks73/cargo-wix
release-windows:
	cargo wix -p rio

lint:
	cargo fmt -- --check --color always
	cargo clippy --all-targets --all-features -- -D warnings

# There is errors regarding null pointers in corcovado that needs to be fixed for Windows
test-win:
	cargo fmt -- --check --color always
	cargo clippy --all-targets --all-features
	RUST_BACKTRACE=full cargo test --release

test:
	make lint
	RUST_BACKTRACE=full cargo test --release

test-renderer:
	cd ./sugarloaf && make test
