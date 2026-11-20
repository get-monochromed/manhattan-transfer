# Upstream source

- [KuickShare](https://github.com/the-beper/KuickShare/tree/ecdf9becef2428577da0a6f5fc3e60b7f660f8aa), GPL-3.0-or-later. Its Quick Share service was adapted for Manhattan Transfer. The upstream notices are in `third_party/KuickShare-NOTICES.md` in the source distribution and `licenses/` in the binary distribution.
- [rquickshare](https://github.com/Martichou/rquickshare), GPL-3.0-only. The code in `third_party/rqs_lib` is the revision vendored by KuickShare, with a build change to supply `protoc` through Cargo. See its separate license and notices.
- [LocalSend](https://github.com/localsend/localsend/tree/e05d12a9c0fd4d8c0245d2ec5c3c177e7e0f759b), Apache-2.0. `third_party/localsend` is its Rust core. `src/core/local_identity.rs` adapts the LocalSend CLI’s certificate handling. See `third_party/localsend/LICENSE`.

The application is distributed under GPL-3.0. `Cargo.lock` records the Rust dependency versions. The executable uses the system Qt 6 libraries and KDE Frameworks QML modules.
