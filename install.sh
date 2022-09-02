#!/bin/sh

if ! command -v tar >/dev/null; then
    echo "Error: tar is required to install rsink" 1>&2
    exit 1
fi

if [ "$OS" = "Windows_NT" ]; then
    target="x86_64-pc-windows-msvc"
else
    case $(uname -sm) in
    "Darwin x86_64") target="x86_64-apple-darwin" ;;
    "Darwin arm64") target="aarch64-apple-darwin" ;;
    "Android i686" | "Android x86" | "Android i786" | "Android i486" | "Android i386") target="i686-linux-android" ;;
    "Android armv7l" | "Android armv8l" | "Android arm") target="armv7-linux-androideabi" ;;
    "Linux aarch64" | "Linux arm64") target="aarch64-unknown-linux-gnu" ;;
    *) target="x86_64-unknown-linux-gnu" ;;
    esac
fi

echo "Target: $target"

arcive_url="https://github.com/abdulrahman1s/RSink/releases/latest/download/rsink-${target}.tar.gz"
install_path="$HOME/.local/bin"
exe="$install_path/rsink"

curl --fail --location --progress-bar --output "$exe.tar.gz" "$arcive_url"
tar -xzvf "$exe.tar.gz" -C "$install_path"
rm "$exe.tar.gz"

if command -v tar >/dev/null; then
    sudo sh -c "echo '[Unit]
Description=Rsink syncls

[Service]
Type=simple
ExecStart=$exe

[Install]
WantedBy=multi-user.target' >> /etc/systemd/system/rsink.service"

    sudo systemctl enable rsink
    sudo systemctl start rsink
    systemctl status rsink
fi

echo "Rsink was installed successfully to $exe"
echo "Please edit/create $HOME/.config/rsink/config.toml to configure the settings"
echo "An example of the configuration file at https://github.com/abdulrahman1s/RSink/blob/master/config.example.toml"
