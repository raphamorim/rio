.PHONY: docs

BUILD_MISC_DIR = misc
TARGET = rio
RELEASE_DIR = target/release
APP_TEMPLATE = $(BUILD_MISC_DIR)/osx/$(APP_NAME)
APP_NAME = Rio.app
APP_DIR = $(RELEASE_DIR)/osx
APP_BINARY = $(RELEASE_DIR)/$(TARGET)
APP_BINARY_DIR = $(APP_DIR)/$(APP_NAME)/Contents/MacOS
APP_EXTRAS_DIR = $(APP_DIR)/$(APP_NAME)/Contents/Resources

all: install run

docs:
	cd docs && make run

run:
	cargo run --release

dev:
	cargo run

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
	@touch -r "$(APP_BINARY)" "$(APP_DIR)/$(APP_NAME)"
	@codesign --remove-signature "$(APP_DIR)/$(APP_NAME)"
	@codesign --force --deep --sign - "$(APP_DIR)/$(APP_NAME)"
	@echo "Created '$(APP_NAME)' in '$(APP_DIR)'"

release-macos: app-universal
	mkdir -p release
	cp -rf ./target/release/osx ./release/
	zip -r ./release/macos-rio.zip ./release/*
	rm -rf ./release/osx

lint:
	cargo fmt -- --check --color always
	cargo clippy --all-targets --all-features -- -D warnings

test:
	make lint
	RUST_BACKTRACE=full cargo test --release

install:
	cargo fetch

build:
	cargo build --release

# Legacy multi build for macOs
# pack-osx-arm:
# 	mkdir -p build
# 	cd rio && cargo bundle --target aarch64-apple-darwin --release --format osx
# 	cp -r ./target/aarch64-apple-darwin/release/bundle/* ./build/macos-arm64/
# 	zip -r ./build/macos-arm64.zip ./build/macos-arm64

# pack-osx-x86:
# 	mkdir -p build
# 	cd rio && cargo bundle --target x86_64-apple-darwin --release --format osx
# 	cp -r ./target/x86_64-apple-darwin/release/bundle/* ./build/macos-x86/
# 	zip -r ./build/macos-x86.zip ./build/macos-x86