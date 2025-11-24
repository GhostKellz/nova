REPO_ROOT := $(abspath ..)
BUILD_ROOT := $(REPO_ROOT)/target/package-build

.PHONY: all arch debian fedora flatpak appimage smoke-test clean

all: arch debian fedora flatpak appimage

arch:
	@echo "[nova] Building Arch package via makepkg"
	@mkdir -p $(BUILD_ROOT)/arch
	@cd $(REPO_ROOT)/packaging/arch && env PKGDEST=$(BUILD_ROOT)/arch makepkg -sf

debian:
	@echo "[nova] Building Debian packages via dpkg-buildpackage"
	@mkdir -p $(BUILD_ROOT)/debian
	@cd $(REPO_ROOT) && sh -c 'set -e; ln -sfn packaging/debian debian; trap "rm -f debian" EXIT; dpkg-buildpackage -us -uc -b -d'
	@find $(REPO_ROOT)/.. -maxdepth 1 -type f -name 'nova-virtualization*.deb' -exec mv -t $(BUILD_ROOT)/debian {} + || true
	@find $(REPO_ROOT)/.. -maxdepth 1 -type f -name 'nova-virtualization-gui*.deb' -exec mv -t $(BUILD_ROOT)/debian {} + || true
	@find $(REPO_ROOT)/.. -maxdepth 1 -type f -name 'nova*.buildinfo' -exec mv -t $(BUILD_ROOT)/debian {} + || true
	@find $(REPO_ROOT)/.. -maxdepth 1 -type f -name 'nova*.changes' -exec mv -t $(BUILD_ROOT)/debian {} + || true

fedora:
	@echo "[nova] Building Fedora RPM via rpmbuild"
	@mkdir -p $(BUILD_ROOT)/fedora/SOURCES
	@cp -r $(REPO_ROOT)/packaging/fedora/* $(BUILD_ROOT)/fedora/SOURCES/
	@rpmbuild -bb $(REPO_ROOT)/packaging/fedora/nova-virtualization.spec \
		--define "_topdir $(BUILD_ROOT)/fedora" \
		--define "_sourcedir $(BUILD_ROOT)/fedora/SOURCES"

flatpak:
	@echo "[nova] Building Flatpak bundle"
	@mkdir -p $(BUILD_ROOT)/flatpak/repo
	@flatpak-builder --force-clean $(BUILD_ROOT)/flatpak/app \
		$(REPO_ROOT)/packaging/flatpak/com.nova.Virtualization.yml \
		--repo=$(BUILD_ROOT)/flatpak/repo

appimage:
	@echo "[nova] Building AppImage via appimage-builder"
	@mkdir -p $(BUILD_ROOT)/appimage
	@cd $(REPO_ROOT) && APPDIR=$(BUILD_ROOT)/appimage appimage-builder \
		--recipe packaging/appimage/AppImageBuilder.yml \
		--skip-test

smoke-test:
	@echo "[nova] Running packaging smoke tests"
	@chmod +x $(REPO_ROOT)/packaging/scripts/smoke-tests.sh
	@$(REPO_ROOT)/packaging/scripts/smoke-tests.sh $(BUILD_ROOT)

clean:
	@echo "[nova] Cleaning packaging build output"
	@rm -rf $(BUILD_ROOT)
