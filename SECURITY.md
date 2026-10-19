# Security Policy

## Hỗ trợ phiên bản

| Version | Supported          |
| ------- | ------------------ |
| `dev` (0.2.x) | :white_check_mark: |
| `main`  | :white_check_mark: |

## Lưu trữ secret

- **Không** lưu Facebook Token, Page Token hay API key trong `frontend`, `localStorage`, `media-index.json`, `content-index.json`, `schedule-index.json` hay git.
- Tất cả secret được mã hóa qua **hệ điều hành**:
  - Windows: Credential Manager (`service: vn.flowpost.desktop`, key `facebook_token`, `fb_page_{id}`, `ai_provider_{id}_key`)
  - macOS: Keychain
  - Linux: Secret Service (libsecret) — fallback file `secure-credentials.json` trong `app_data_dir` với ghi atomic `json.tmp` → `rename`
- Frontend chỉ biết trạng thái `hasFacebookToken`/`hasApiKey` qua `credential_status` / `get_active_provider.hasApiKey`, không bao giờ nhận giá trị thô.
- `facebook.rs:154` cache Page token sau `list_facebook_pages` vào keyring `fb_page_{id}`, không trả về `PageInfo` có `access_token`.
- `publish_content` ưu tiên `fb_page_{page_id}` trước `facebook_token`, lỗi rõ ràng `Không tìm thấy Page/User token, vui lòng Tải Trang lại`.
- Ảnh `publish_content` giới hạn `10MB` và whitelist mime `jpeg/png/webp/gif` qua `mime_guess` trước khi đọc file.

## Báo cáo lỗ hổng

Nếu bạn phát hiện lỗ hổng bảo mật, vui lòng **không** tạo issue công khai. Gửi email tới maintainer với tiêu đề `[SECURITY] FlowPost AI` kèm:
- Mô tả, bước tái hiện, ảnh hưởng
- Commit hash (`git log --oneline -1`) và OS (Windows/macOS/Linux)
- Chúng tôi sẽ phản hồi trong 72h và phối hợp fix trước khi công khai

## Lịch sử key

- **P0 audit (tháng 9/2026) + remediation (2026-10-19):** Lịch sử git đã được viết lại bằng `git filter-repo`. Commit gốc chứa key không còn tồn tại trong repo, và quét mọi ref reachable (`git log --all -S"sk-"`) không còn `sk-*` key thật — chỉ còn placeholder `sk-REPLACE_WITH_YOUR_KEY` trong code và tài liệu.
- **Lưu ý `.env`:** `.env` không được track; chỉ `e2e/.gitignore` bỏ qua `.env` (`.gitignore` gốc thì không). Nếu máy developer có `e2e/.env` cục bộ thì file đó không nằm trong lịch sử git; mỗi người tự quản lý và luân chuyển secret cục bộ.
- Khuyến nghị tiếp tục dùng biến môi trường cho mọi secret như best practice, không commit `.env` hay API key trực tiếp.

## Khuyến nghị vận hành

- Luôn dùng `E2E_API_KEY` qua biến môi trường `env:E2E_API_KEY`, không commit `.env` (đã gitignore trong `e2e/.gitignore`).
- Quay vòng Page token khi hết hạn (Graph API trả `code 190`) — vào `Cài đặt → Tải Trang` lại.
- Không log `access_token` ra console hay file; `map_graph_error` chỉ log `code` + `message` đã cắt 300 ký tự.
