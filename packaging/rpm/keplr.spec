Name:           keplr
Version:        %{keplr_version}
Release:        1%{?dist}
Summary:        Personal lightweight IDE
License:        MIT
URL:            https://github.com/NaveenSingh9999/keplr
BuildArch:      %{keplr_arch}

%description
Keplr is a personal lightweight IDE in a single binary: tree-sitter
highlighting, web UI, PTY terminals, serial monitor, source control,
headless serve plus desktop and TUI, Git LFS aware, CRDT sync.

%install
mkdir -p %{buildroot}%{_bindir}
mkdir -p %{buildroot}%{_datadir}/applications
install -D -m755 %{keplr_bin} %{buildroot}%{_bindir}/keplr
install -D -m644 %{keplr_desktop} %{buildroot}%{_datadir}/applications/keplr.desktop
install -D -m644 %{keplr_icons}/keplr-16.png %{buildroot}%{_datadir}/icons/hicolor/16x16/apps/keplr.png
install -D -m644 %{keplr_icons}/keplr-32.png %{buildroot}%{_datadir}/icons/hicolor/32x32/apps/keplr.png
install -D -m644 %{keplr_icons}/keplr-48.png %{buildroot}%{_datadir}/icons/hicolor/48x48/apps/keplr.png
install -D -m644 %{keplr_icons}/keplr-128.png %{buildroot}%{_datadir}/icons/hicolor/128x128/apps/keplr.png
install -D -m644 %{keplr_icons}/keplr-256.png %{buildroot}%{_datadir}/icons/hicolor/256x256/apps/keplr.png
install -D -m644 %{keplr_icons}/keplr-512.png %{buildroot}%{_datadir}/icons/hicolor/512x512/apps/keplr.png

%files
%{_bindir}/keplr
%{_datadir}/applications/keplr.desktop
%{_datadir}/icons/hicolor/*/apps/keplr.png

%changelog
* Sun Sep 20 2026 Keplr <keplr@example.com> - 0.1.0-1
- First public release
