# Windows compatibility checks

These component checks do not build or verify the complete Atlas application. Windows workspace storage, protected workspaces, native services, model runtime, FAISS, dialogs and packaging need separate Windows implementations and testing.

Run the download checks with the repository root as the argument. They use synthetic data and a local HTTP server; Python 3, curl, Rust, and cached `libc` and `sha2` dependencies are required. The second check exercises Windows-specific logic on a Unix host with a test curl, not the Windows executable:

```sh
python3 tests/windows/check_unix_model_download.py "$PWD"
python3 tests/windows/check_model_download.py "$PWD"
```

With the Rust `x86_64-pc-windows-gnu` target, MinGW compiler and dependencies already cached, compile the Windows downloader code and its tests:

```sh
sh tests/windows/check_model_download_compile.sh "$PWD"
mkdir -p /tmp/atlas-windows-test-output
sh tests/windows/build_model_download_tests.sh "$PWD" /tmp/atlas-windows-test-output
```

Run the resulting executable on Windows or in an isolated compatibility environment. Its tests cover existing-file protection and hash checking, not a complete network download using Windows curl. The proxy security test accepts the path to a native binary, or a compatibility runner followed by a Windows binary:

```sh
python3 native/atlas-native-proxy/tests/test_proxy.py /path/to/atlas-native-proxy
```

Real Windows validation remains necessary for system curl, NTFS, access permissions, WebView2 and the complete application.

# Real Windows model download contract test

This isolated check builds the six actual Atlas downloader modules with only the download URL and SHA-256 constant changed to point at synthetic data from a local loopback HTTP server. It exercises Windows System32 curl, checksum checking, cancellation, existing destination protection and concurrent creation (6 modules × 5 cases). It needs an available 127.0.0.1 port 47877; do not run multiple copies in parallel. It does not download the real models, test TLS, simulate machine shutdown or verify full Atlas.

Requirements: Python 3, Rust x86_64-pc-windows-gnu target, MinGW x86_64 C compiler, internet access or Cargo dependency cache. On a Unix development host:

```sh
python3 tests/windows/prepare_vm_download_tests.py "$PWD" /tmp/atlas-download-win-test
cd /tmp/atlas-download-win-test
cargo build --target x86_64-pc-windows-gnu
```

Copy `target/x86_64-pc-windows-gnu/debug/atlas-download-vm-test.exe` from the generated test directory and `tests/windows/run_vm_download_tests.ps1` from the Atlas repository to a Windows machine. In PowerShell:

```powershell
.\run_vm_download_tests.ps1 -Executable .\atlas-download-vm-test.exe
```

The runner creates a unique directory in Windows TEMP and removes only that directory on completion or test failure. It reports all 30 cases and exits on the first failure. It expects real Windows environment variables, particularly SystemRoot, and an NTFS-compatible file system; do not override SystemRoot to simulate Windows.
