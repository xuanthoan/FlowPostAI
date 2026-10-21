# Security Policy

## Hỗ trợ phiên bản

| Version | Supported          |
| ------- | ------------------ |
| `dev` (0.2.x) | :white_check_mark: |
| `main`  | :white_check_mark: |

## Lưu trữ secret

- Secret (Facebook User/Page token, API key của provider) được ghi vào keyring của hệ điều hành: Windows Credential Manager (`service: vn.flowpost.desktop`, key `facebook_token`, `fb_page_{id}`, `ai_provider_{id}_key`), macOS Keychain, Linux Secret Service (libsecret).
- **Hiện trạng quan trọng:** Tính đến giai đoạn này, ứng dụng ghi bản sao dự phòng plaintext tại secure-credentials.json trên mọi hệ điều hành, kể cả khi keyring thành công. Cơ chế: `security.rs:set_secret_hybrid` ghi fallback vô điều kiện mỗi khi có `AppHandle`, rồi mới trả về kết quả keyring — đây không phải hành vi chỉ dành cho Linux.
- Hệ quả: bất kỳ tiến trình nào chạy dưới cùng user OS, cùng các công cụ backup, EDR/AV đều có thể đọc file đó. File không được mã hóa thêm và hiện chưa có biện pháp giảm thiểu nào khác.
- Hành vi dự kiến (**Phase 4, CHƯA triển khai**): chỉ ghi fallback khi keyring thất bại.
- Các lệnh trạng thái `credential_status` / `get_active_provider.hasApiKey` chỉ cho biết đã có secret hay chưa, không trả giá trị. Ngoại lệ: Lệnh get_facebook_token hiện vẫn trả về token thô cho webview; frontend không sử dụng lệnh này. Xem `security.rs:get_facebook_token`.
- `facebook.rs:list_facebook_pages` cache Page token vào keyring `fb_page_{id}` và không trả về `PageInfo` có `access_token`.
- `publish_content` ưu tiên `fb_page_{page_id}` trước `facebook_token`, lỗi rõ ràng `Không tìm thấy Page/User token, vui lòng Tải Trang lại`.
- Ảnh `publish_content` giới hạn `10MB` và whitelist mime `jpeg/png/webp/gif` qua `mime_guess` trước khi đọc file.
- Token không được ghi vào `media-index.json`, `content-index.json` hay git. Ngoại lệ đã biết: `schedule-index.json` có thể chứa token qua trường `lastError` (xem mục **Khuyến nghị vận hành**).

## Báo cáo lỗ hổng

Nếu bạn phát hiện lỗ hổng bảo mật, vui lòng **không** tạo issue công khai. Gửi email tới maintainer với tiêu đề `[SECURITY] FlowPost AI` kèm:
- Mô tả, bước tái hiện, ảnh hưởng
- Commit hash (`git log --oneline -1`) và OS (Windows/macOS/Linux)
- Chúng tôi sẽ phản hồi trong 72h và phối hợp fix trước khi công khai

## Lịch sử key

- **P0 audit (tháng 9/2026) + remediation (2026-10-19):** Lịch sử git đã được viết lại bằng `git filter-repo`. Commit gốc chứa key không còn tồn tại trong repo, và quét mọi ref reachable (`git log --all -S"sk-"`) không còn `sk-*` key thật — chỉ còn placeholder `sk-REPLACE_WITH_YOUR_KEY` trong code và tài liệu.
- **Lưu ý `.env`:** `.env` không được track; chỉ `e2e/.gitignore` bỏ qua `.env`, còn `.gitignore` ở gốc repo vẫn **không** bỏ qua `.env`. Việc bổ sung `.env` vào `.gitignore` gốc là **Phase 4 (chưa triển khai)**. Nếu máy developer có `e2e/.env` cục bộ thì file đó không nằm trong lịch sử git; mỗi người tự quản lý và luân chuyển secret cục bộ.
- Khuyến nghị tiếp tục dùng biến môi trường cho mọi secret như best practice, không commit `.env` hay API key trực tiếp.

## Khuyến nghị vận hành

- Luôn dùng `E2E_API_KEY` qua biến môi trường `env:E2E_API_KEY`, không commit `.env` (đã gitignore trong `e2e/.gitignore`).
- Quay vòng Page token khi hết hạn (Graph API trả `code 190`) — vào `Cài đặt → Tải Trang` lại.
- Với lỗi HTTP có body JSON, `map_graph_error` trả `code` + `message` nguyên văn; chỉ nhánh không parse được JSON mới cắt body 300 ký tự.
- **Known issue (chưa sửa):** các nhánh lỗi mạng format nguyên `reqwest::Error`; Display của nó chứa URL request, mà URL hiện mang `access_token` trong query string. Chuỗi này được hiển thị trong UI và lưu vào `lastError` của schedule index. Vì vậy `schedule-index.json` có thể chứa token trên máy đã từng đăng bài. Cách sửa (**Phase 4**): làm sạch (sanitize) lỗi trước khi lưu và hiển thị.
