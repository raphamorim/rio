.PHONY: docs

BUILD_MISC_DIR = misc
DOCS_DIR = docs
TARGET = rio
TARGET_DIR = target/release
TARGET_DIR_DEBIAN = target/debian
TARGET_DIR_OSX = $(TARGET_DIR)/osx
RELEASE_DIR = release

APP_NAME = Rio.app
APP_TEMPLATE = $(BUILD_MISC_DIR)/osx/$(APP_NAME)
APP_BINARY = $(TARGET_DIR)/$(TARGET)
APP_BINARY_DIR = $(TARGET_DIR_OSX)/$(APP_NAME)/Contents/MacOS
APP_EXTRAS_DIR = $(TARGET_DIR_OSX)/$(APP_NAME)/Contents/Resources
TERMINFO = $(BUILD_MISC_DIR)/rio.terminfo

all: install run

docs:
	cd $(DOCS_DIR) && npm start

docs-build:
	cd $(DOCS_DIR) && npm ci && npm run build

run:
	cargo run --release

dev:
	cargo run -p rioterm

dev-wa:
	cargo run -p rioterm

dev-debug:
	RIO_LOG_LEVEL=debug make dev

dev-debug-wayland:
	RIO_LOG_LEVEL=debug cargo run -p rioterm --no-default-features --features=wayland

dev-debug-x11:
	RIO_LOG_LEVEL=debug cargo run -p rioterm --no-default-features --features=x11

run-wasm:
	cargo build -p rioterm --target wasm32-unknown-unknown --lib
	cd rio-wasm && make run

dev-watch:
	#cargo install cargo-watch
	cargo watch -- cargo run -p rioterm

install:
	cargo fetch

build: install
	RUSTFLAGS='-C link-arg=-s' cargo build --release

# install:
# rustup target add x86_64-apple-darwin
# rustup target add aarch64-apple-darwin
$(TARGET)-universal:
	RUSTFLAGS='-C link-arg=-s' MACOSX_DEPLOYMENT_TARGET="10.11" cargo build --release --target=x86_64-apple-darwin
	RUSTFLAGS='-C link-arg=-s' MACOSX_DEPLOYMENT_TARGET="10.11" cargo build --release --target=aarch64-apple-darwin
	@lipo target/{x86_64,aarch64}-apple-darwin/release/$(TARGET) -create -output $(APP_BINARY)

app-universal: $(APP_NAME)-universal ## Create a universal Rio.app
$(APP_NAME)-%: $(TARGET)-%
	@mkdir -p $(APP_BINARY_DIR)
	@mkdir -p $(APP_EXTRAS_DIR)
	@cp -fRp $(APP_TEMPLATE) $(TARGET_DIR_OSX)
	@cp -fp $(APP_BINARY) $(APP_BINARY_DIR)
	@touch -r "$(APP_BINARY)" "$(TARGET_DIR_OSX)/$(APP_NAME)"

install-terminfo:
	@tic -xe rio -o $(APP_EXTRAS_DIR) $(TERMINFO)

release-macos: app-universal
	@codesign --remove-signature "$(TARGET_DIR_OSX)/$(APP_NAME)"
	@codesign --force --deep --sign - "$(TARGET_DIR_OSX)/$(APP_NAME)"
	@echo "Created '$(APP_NAME)' in '$(TARGET_DIR_OSX)'"
	mkdir -p $(RELEASE_DIR)
	cp -rf ./target/release/osx/* ./release/
	cd ./release && zip -r ./macos-unsigned.zip ./*

release-macos-signed:
	$(eval VERSION = $(shell echo $(version)))
	$(if $(strip $(VERSION)),make release-macos-signed-app, make version-not-found)

release-macos-signed-app:
	@make app-universal
	@echo "Releasing Rio v$(version)"
	@codesign --force --deep --options runtime --sign "Developer ID Application: Hugo Amorim" "$(TARGET_DIR_OSX)/$(APP_NAME)"
	mkdir -p $(RELEASE_DIR) && cp -rf ./target/release/osx/* ./release/
	@ditto -c -k --keepParent ./release/$(APP_NAME) ./release/Rio-v$(version).zip
	@xcrun notarytool submit ./release/Rio-v$(version).zip --keychain-profile "Hugo Amorim" --wait
	rm -rf ./release/$(APP_NAME)
	@unzip ./release/Rio-v$(version).zip -d ./release

install-macos: release-macos
	rm -rf /Applications/$(APP_NAME)
	mv ./release/$(APP_NAME) /Applications/

version-not-found:
	@echo "Rio version was not specified"
	@echo " - usage: $ make release-macos-signed version=0.0.0"

# e.g: make update-version old-version=0.0.13 new-version=0.0.12
update-version:
	@echo "Switching from $(old-version) to $(new-version)"
	find Cargo.toml -type f -exec sed -i '' 's/$(old-version)/$(new-version)/g' {} \;
	find CHANGELOG.md -type f -exec sed -i '' 's/Unreleased/Unreleased\n\n- TBD\n\n## $(new-version)/g' {} \;
	find $(BUILD_MISC_DIR)/windows/rio.wxs -type f -exec sed -i '' 's/$(old-version)/$(new-version)/g' {} \;
	find $(APP_TEMPLATE)/Contents/Info.plist -type f -exec sed -i '' 's/$(old-version)/$(new-version)/g' {} \;

release-macos-dmg:
# 	Using https://www.npmjs.com/package/create-dmg
	cd ./release && create-dmg $(APP_NAME) --dmg-title="Rio ${version}" --overwrite

bump-brew:
	brew bump-cask-pr rio --version ${version}

# TODO: Move to bin path
release-x11:
	RUSTFLAGS='-C link-arg=-s' cargo build --release --no-default-features --features=x11
	target/release/rio
release-wayland:
	RUSTFLAGS='-C link-arg=-s' cargo build --release --no-default-features --features=wayland
	target/release/rio

# Debian
# cargo install cargo-deb
# To install: sudo release/debian/rio_<version>_<architecture>_<feature>.deb
# e.g: sudo release/debian/rio_0.0.13_arm64_wayland.deb
release-debian-x11:
	cargo deb -p rioterm -- --release --no-default-features --features=x11
	mkdir -p $(RELEASE_DIR)/debian/x11
	mv $(TARGET_DIR_DEBIAN)/* $(RELEASE_DIR)/debian/x11/
	cd $(RELEASE_DIR)/debian/x11 && rename 's/.deb/_x11.deb/g' *

release-debian-wayland:
	cargo deb -p rioterm -- --release --no-default-features --features=wayland
	mkdir -p $(RELEASE_DIR)/debian/wayland
	mv $(TARGET_DIR_DEBIAN)/* $(RELEASE_DIR)/debian/wayland/
	cd $(RELEASE_DIR)/debian/wayland && rename 's/.deb/_wayland.deb/g' *

# Release and Install
install-debian-x11:
	cargo install cargo-deb
	cargo deb -p rioterm --install -- --release --no-default-features --features=x11
install-debian-wayland:
	cargo install cargo-deb
	cargo deb -p rioterm --install -- --release --no-default-features --features=wayland

# cargo install cargo-wix
# https://github.com/volks73/cargo-wix
release-windows:
	cargo wix -p rioterm

# Note had to separate clippy default command because rio-backend feature
# flag has been conflicting in the checks
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

publish-crates:
	cargo build --release
	cargo publish -p rio-proc-macros
	cargo publish -p copa
	cargo publish -p corcovado
	cargo publish -p teletypewriter
	cargo publish -p sugarloaf
	cargo publish -p rio-backend
	cargo publish -p rioterm

test-renderer:
	cd ./sugarloaf && make test
