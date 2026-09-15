# E2E với server thật

## Chạy server thật trước
```bash
# Server phải lắng tại http://127.0.0.1:20128/v1
curl -H "Authorization: Bearer $E2E_API_KEY" http://127.0.0.1:20128/v1/models
```

## Chạy E2E
```bash
# Cài tauri-driver (một lần)
cargo install tauri-driver

# Cài deps đã có @playwright/test
npm install

# Chạy Tauri + Playwright (cần 2 terminal hoặc dùng webServer)
# Terminal 1:
npm run tauri:dev

# Terminal 2 (đặt env, không commit key):
$env:E2E_API_KEY="sk-REPLACE_WITH_YOUR_KEY"
$env:E2E_BASE_URL="http://127.0.0.1:20128/v1"
npx playwright test
# hoặc
npm run e2e
```

## Lưu ý
- Tests skip tự động nếu `E2E_API_KEY` không set hoặc không chạy trong Tauri (`__TAURI_INTERNALS__` missing).
- Key không được commit, chỉ đọc từ env `E2E_API_KEY`.
- Server phải chạy trước khi test, nếu không sẽ fail `Không thể kết nối AI provider`.
