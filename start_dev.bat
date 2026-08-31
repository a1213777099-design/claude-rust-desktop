@echo off
set CARGO_HOME=F:\rust-cache\.cargo
set PATH=F:\rust-cache\.cargo\bin;%PATH%
cd /d F:\Projects\claude-code-rust
npm run tauri dev
