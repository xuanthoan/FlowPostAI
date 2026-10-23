# Security Policy

## Hỗ trợ phiên bản

| Version | Supported          |
| ------- | ------------------ |
| `dev` (0.2.x) | :white_check_mark: |
| `main`  | :white_check_mark: |

## Lưu trữ secret

- Secret (Facebook User/Page token, API key của provider) hiện được lưu trong file fallback `secure-credentials.json` tại `app_data_dir` (Windows: `%APPDATA%\vn.flowpost.desktop`), dạng plaintext, quyền truy cập theo user đang chạy app. Bản build này **chưa bật backend keyring gốc** của hệ điều hành; khi được bật, secret sẽ dùng Windows Credential Manager (`service: vn.flowpost.desktop`, key `facebook_token`, `fb_page_{id}`, `ai_provider_{id}_key`).
- **Chính sách hiện tại:** `security.rs:set_secret_hybrid` thử ghi keyring trước, nhưng chỉ coi là thành công khi **đọc lại được** giá trị vừa ghi bằng một `Entry` mới. Nếu không đọc lại được (ví dụ build chưa có backend nền tảng), giá trị được ghi vào fallback plaintext và fallback được giữ lại; chỉ khi keyring ghi và đọc lại thành công thì bản fallback của key đó mới được xóa (best-effort). Nhờ vậy không còn trường hợp ghi keyring "thành công" giả rồi xóa mất bản fallback.
- Giá trị cũ vẫn được đọc: `get_secret_hybrid` đọc keyring trước, chỉ dùng fallback khi keyring `NoEntry` hoặc lỗi nền tảng. Khi keyring `NoEntry` mà fallback còn giá trị, ứng dụng thử migrate sang keyring bằng cùng quy tắc read-back rồi mới xóa fallback; nếu đọc keyring thành công và fallback trùng giá trị thì fallback được xóa, nếu khác giá trị thì giữ nguyên (không ghi đè keyring).
- Hệ quả: khi keyring hỏng hoàn toàn, fallback vẫn có thể tồn tại và bất kỳ tiến trình nào chạy dưới cùng user OS, cùng công cụ backup, EDR/AV đều có thể đọc file đó. File không được mã hóa thêm. Máy đã chạy bản cũ vẫn có thể còn bản sao plaintext cho tới lần ghi/đọc kế tiếp — nên kiểm tra và quay vòng token nếu nghi ngờ.
- Các lệnh trạng thái `credential_status` / `get_active_provider.hasApiKey` chỉ cho biết đã có secret hay chưa, không trả giá trị. Lệnh `get_facebook_token` đã bị xóa; frontend không còn API nào trả token thô.
- `facebook.rs:list_facebook_pages` cache Page token vào keyring `fb_page_{id}` và không trả về `PageInfo` có `access_token`.
- Mọi request Graph dùng header `Authorization: Bearer`; token không còn nằm trong URL, form body hay multipart body.
- `publish_content` ưu tiên `fb_page_{page_id}` trước `facebook_token`, lỗi rõ ràng `Không tìm thấy Page/User token, vui lòng Tải Trang lại`. `page_id`/`post_id` được kiểm tra chỉ gồm chữ số trước khi gọi mạng (chặn chèn path/query vào URL).
- Ảnh `publish_content` giới hạn `10MB` và whitelist mime `jpeg/png/webp/gif` qua `mime_guess` trước khi đọc file.
- Lỗi Graph được làm sạch trước khi trả về frontend hoặc ghi `lastError`: `reqwest::Error::without_url()` bỏ URL request, sau đó `scrub_error_text` thay token chứa URL bằng `[url]` và token chứa `access_token`/`bearer` bằng `[redacted]`; nhánh body không phải JSON cũng được scrub trước khi cắt 300 ký tự.
- Token không được ghi vào `media-index.json`, `content-index.json` hay git. `schedule-index.json` không còn nhận token qua `lastError` nhờ bước scrub; file do phiên bản cũ ghi vẫn có thể còn token và cần được kiểm tra/quay vòng thủ công.

## Báo cáo lỗ hổng

Nếu bạn phát hiện lỗ hổng bảo mật, vui lòng **không** tạo issue công khai. Gửi email tới maintainer với tiêu đề `[SECURITY] FlowPost AI` kèm:
- Mô tả, bước tái hiện, ảnh hưởng
- Commit hash (`git log --oneline -1`) và OS (Windows/macOS/Linux)
- Chúng tôi sẽ phản hồi trong 72h và phối hợp fix trước khi công khai

## Lịch sử key

- **P0 audit (tháng 9/2026) + remediation (2026-10-19):** Lịch sử git đã được viết lại bằng `git filter-repo`. Commit gốc chứa key không còn tồn tại trong repo, và quét mọi ref reachable (`git log --all -S"sk-"`) không còn `sk-*` key thật — chỉ còn placeholder `sk-REPLACE_WITH_YOUR_KEY` trong code và tài liệu.
- **Lưu ý `.env`:** `.env` và `.env.*` đã được thêm vào `.gitignore` ở gốc repo (vẫn giữ `.env.example`); `e2e/.gitignore` cũng bỏ qua `.env`. Nếu máy developer có `.env` cục bộ thì file đó không nằm trong lịch sử git; mỗi người tự quản lý và luân chuyển secret cục bộ.
- Khuyến nghị tiếp tục dùng biến môi trường cho mọi secret như best practice, không commit `.env` hay API key trực tiếp.

## Khuyến nghị vận hành

- Luôn dùng `E2E_API_KEY` qua biến môi trường `env:E2E_API_KEY`, không commit `.env` (đã gitignore trong `e2e/.gitignore`).
- Quay vòng Page token khi hết hạn (Graph API trả `code 190`) — vào `Cài đặt → Tải Trang` lại.
- Với lỗi HTTP có body JSON, `map_graph_error` trả `code` + `message`; nhánh body không parse được JSON sẽ được scrub URL/token rồi mới cắt 300 ký tự.
- **Đã xử lý (Phase 4):** các nhánh lỗi mạng dùng `reqwest::Error::without_url()` kết hợp `scrub_error_text` trước khi trả về UI hoặc ghi `lastError`, nên `schedule-index.json` không còn nhận token mới. File do bản cũ ghi vẫn có thể chứa token — hãy mở và kiểm tra thủ công, sau đó quay vòng Page token nếu cần.
