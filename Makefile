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
	RUSTFLAGS='-C link-arg=-s' cargo build --release

$(TARGET)-universal:
	RUSTFLAGS='-C link-arg=-s' MACOSX_DEPLOYMENT_TARGET="10.11" cargo build --release --target=x86_64-apple-darwin
	RUSTFLAGS='-C link-arg=-s' MACOSX_DEPLOYMENT_TARGET="10.11" cargo build --release --target=aarch64-apple-darwin
	@lipo target/{x86_64,aarch64}-apple-darwin/release/$(TARGET) -create -output $(APP_BINARY)

app-universal: $(APP_NAME)-universal ## Create a universal Rio.app
$(APP_NAME)-%: $(TARGET)-%
	@mkdir -p $(APP_BINARY_DIR)
	@mkdir -p $(APP_EXTRAS_DIR)
	@cp -fRp $(APP_TEMPLATE) $(APP_DIR)
	@cp -fp $(APP_BINARY) $(APP_BINARY_DIR)
	@tic -xe rio -o $(APP_EXTRAS_DIR) $(TERMINFO)
	@touch -r "$(APP_BINARY)" "$(APP_DIR)/$(APP_NAME)"

release-macos: app-universal
	@codesign --remove-signature "$(APP_DIR)/$(APP_NAME)"
	@codesign --force --deep --sign - "$(APP_DIR)/$(APP_NAME)"
	@echo "Created '$(APP_NAME)' in '$(APP_DIR)'"
	mkdir -p release
	cp -rf ./target/release/osx/* ./release/
	cd ./release && zip -r ./macos-rio.zip ./*

version-not-found:
	@echo "Rio version was not specified"
	@echo " - usage: $ make release-macos-signed version=0.0.0"

release-macos-app-signed:
	@make app-universal
	@echo "Releasing Rio v$(version)"
	@codesign --force --deep --options runtime --sign "Developer ID Application: Hugo Amorim" "$(APP_DIR)/$(APP_NAME)"
	mkdir -p release && cp -rf ./target/release/osx/* ./release/
	@ditto -c -k --keepParent ./release/Rio.app ./release/Rio-v$(version).zip
	@xcrun notarytool submit ./release/Rio-v$(version).zip --keychain-profile "Hugo Amorim" --wait
	rm -rf ./release/Rio.app
	@unzip ./release/Rio-v$(version).zip -d ./release

release-macos-dmg:
# 	Using https://www.npmjs.com/package/create-dmg
	cd ./release && create-dmg Rio.app --dmg-title="Rio ${version}" --overwrite

# 	Using https://github.com/create-dmg/create-dmg
# 	create-dmg \
# 		--volname "Rio" \
#   		--volicon "$(APP_EXTRAS_DIR)/Rio-stable.icns" \
# 		--text-size 30 \
# 		--window-pos 200 120 \
# 		--window-size 800 400 \
# 		--icon-size 100 \
# 		--icon "Rio.app" 200 190 \
#   		--hide-extension "Rio.app" \
#   		--app-drop-link 600 185 \
#   		--skip-jenkins \
# 		--background "./resources/rio-colors.png" \
# 		./release/Rio-v0.0.0.dmg ./release/Rio.app
# 	mv "./release/Rio $(version).dmg" "./release/Rio-v$(version).dmg"

release-macos-signed:
	$(eval VERSION = $(shell echo $(version)))
	$(if $(strip $(VERSION)),make release-macos-app-signed, make version-not-found)

# TODO: Move to bin path
release-x11:
	RUSTFLAGS='-C link-arg=-s' cargo build --release --no-default-features --features=x11
	WINIT_UNIX_BACKEND=x11 target/release/rio
release-wayland:
	RUSTFLAGS='-C link-arg=-s' cargo build --release --no-default-features --features=wayland
	WINIT_UNIX_BACKEND=wayland target/release/rio

# Debian
# cargo install cargo-deb
# output: target/debian/*
# To install: sudo target/debian/rio_<version>_<architecture>.deb
release-debian-x11:
	cargo deb -p rio --deb-version="x11" -- --release --no-default-features --features=x11
release-debian-wayland:
	cargo deb -p rio --deb-version="wayland" -- --release --no-default-features --features=wayland
# Release and Install
install-debian-x11:
	cargo install cargo-deb
	cargo deb -p rio --install -- --release --no-default-features --features=x11
install-debian-wayland:
	cargo install cargo-deb
	cargo deb -p rio --install -- --release --no-default-features --features=wayland

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
