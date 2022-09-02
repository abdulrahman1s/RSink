#!/bin/sh

if ! command -v tar >/dev/null; then
    echo "Error: tar is required to install rsink" 1>&2
    exit 1
fi

case $(uname -s) in
"Android") os="linux-android" ;;
"Linux") os="unknown-linux-gnu" ;;
"Darwin") os="apple-darwin" ;;
esac

if command -v termux-setup-storage; then
    os="linux-android"
fi

case $(uname -m) in
i386 | i486 | i686 | i786 | x86)
    arch=i686
    ;;
xscale | arm)
    arch=arm
    if [ "$os" = "linux-android" ]; then
        os=linux-androideabi
    fi
    ;;
armv6l)
    arch=arm
    if [ "$os" = "linux-android" ]; then
        os=linux-androideabi
    else
        os="${os}eabihf"
    fi
    ;;
armv7l | armv8l)
    arch=armv7
    if [ "$os" = "linux-android" ]; then
        os=linux-androideabi
    else
        os="${os}eabihf"
    fi
    ;;
aarch64 | arm64)
    arch=aarch64
    ;;
x86_64 | x86-64 | x64 | amd64)
    arch=x86_64
    ;;
*)
    echo "unknown CPU type: $arch"
    exit 1
    ;;
esac


echo "Target: $arch-$os"

arcive_url="https://github.com/abdulrahman1s/RSink/releases/latest/download/rsink-$arch-$os.tar.gz"
install_path="$HOME/.local/bin"
exe="$install_path/rsink"

# For termux users
mkdir -p "$install_path"
curl --fail --location --progress-bar --output "$exe.tar.gz" "$arcive_url"
tar -xzvf "$exe.tar.gz" -C "$install_path"
rm "$exe.tar.gz"

if command -v termux-setup-storage; then
    mkdir -p ~/.termux/boot
    echo "#!/data/data/com.termux/files/usr/bin/sh 
    $exe" > ~/.termux/boot/rsink.sh
fi

if command -v systemctl >/dev/null; then
    mkdir -p ~/.config/systemd/user/
    systemctl --user disable rsink
    echo "[Unit]
Description=Rsink syncls
After=network.target

[Service]
Type=simple
ExecStart=$exe
Restart=always

[Install]
WantedBy=multi-user.target" >~/.config/systemd/user/rsink.service
    systemctl --user enable rsink
    systemctl --user start rsink
    systemctl --user status rsink
fi

echo "Rsink was installed successfully to $exe"
echo "Please edit/create $HOME/.config/rsink/config.toml to configure the settings"
echo "An example of the configuration file at https://github.com/abdulrahman1s/RSink/blob/master/config.example.toml"
