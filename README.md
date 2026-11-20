# Manhattan Transfer 0.2.1

Manhattan Transfer sends and receives files over Quick Share and LocalSend from one Qt 6 and KDE Kirigami window. The Linux x86-64 bundle was built for Fedora 45 with Plasma 6.

## Run

Open a terminal in the bundle and run `./Launch\ Manhattan\ Transfer`. The launcher starts both transfer engines. You can use the app directly from the bundle.

To add it to your application menu and Dolphin’s context menu, run `./install.sh`. This installs to `~/.local` and uses the existing Plasma session. Enable automatic startup with `systemctl --user daemon-reload` followed by `systemctl --user enable --now nearby-transfer.service`. The app starts in the tray when you sign in to Plasma.

## Send and receive

- **Send** supports choosing or dropping files and folders. A full successful send clears the items in that batch and preserves anything added while it was sending. A failed, declined, cancelled, or partial send keeps the batch in the queue. Put the phone on the same Wi-Fi. Open Quick Share’s **Receive** screen or open LocalSend on the destination device, then select it in Manhattan Transfer. Quick Share devices appear first.
- Folder transfers use LocalSend and preserve named directories containing files. The LocalSend wire format describes files and their relative paths. Empty directories and links inside folders are outside that format.
- **Try with LocalSend** appears after an outgoing Quick Share connection failure. It keeps the original file selection and shows LocalSend destinations. Open LocalSend on the other device and select the destination to retry. If you know its IP address, **Enter LocalSend address** can find it without multicast discovery.
- **Receive** displays the name to look for on your phone. Android can share with the laptop directly through its built-in Quick Share menu. To fall back from the phone, share the files through the LocalSend app. Both routes show a Plasma notification with **Accept** and **Decline**. A Quick Share verification code appears in the notification when the sender provides one. Incoming files save to `Downloads/Nearby Transfer` by default.
- Closing the window keeps the service in the Plasma tray when available. Choose **Quit** in the tray menu to stop both listeners. The **Visible** switch controls Quick Share discovery.
- **Transfers** shows the progress and outcome of transfers during the current session. LocalSend recipients configured to require a PIN are outside the current send workflow.

## Network requirements

Quick Share uses the shared Wi-Fi network with multicast DNS and uses Bluetooth for discovery. LocalSend uses UDP `53317` for discovery and HTTPS on TCP `53317` by default. If the original LocalSend app already has TCP `53317`, Manhattan Transfer uses an available port and advertises it. When verifying discovery, quit the original Packet and LocalSend apps from their trays to avoid seeing the old desktop names as well.

You can set `NEARBY_DEVICE_NAME`, `NEARBY_DOWNLOAD_DIR`, `NEARBY_QUICK_SHARE_PORT`, or `NEARBY_LOCALSEND_PORT` before starting the launcher. Set a fixed Quick Share port if your firewall requires a specific inbound rule.

## Build from source

Build on Linux with Cargo and a recent stable Rust compiler, CMake 3.24 or newer, a C++20 compiler, Qt 6 development headers with Qt Quick, and D-Bus development headers. Running the UI also requires KDE Kirigami and the KDE desktop style for Qt Quick Controls. The source includes the two engines and a pinned `Cargo.lock`.

Run `./build.sh`. It writes an executable bundle to `dist/Manhattan Transfer`. You can provide another output directory as its first argument.

See [THIRD_PARTY.md](THIRD_PARTY.md) for the vendored source and licenses.
