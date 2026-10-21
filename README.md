# FlowPost AI Desktop

[![CI](https://github.com/xuanthoan/auto_creat_content_public_fb/actions/workflows/ci.yml/badge.svg?branch=dev)](https://github.com/xuanthoan/auto_creat_content_public_fb/actions/workflows/ci.yml)

FlowPost AI là ứng dụng desktop React + Tauri để chuẩn bị nội dung Facebook. Bản hiện tại bao gồm dashboard và kho media lưu trực tiếp trên ổ cứng; media không được tải lên máy chủ bên thứ ba.

## Yêu cầu

- Node.js 20 trở lên.
- Rust stable và Cargo.
- Các system dependency của Tauri 2 dành cho hệ điều hành đang sử dụng.

Hướng dẫn cài system dependency chính thức: <https://v2.tauri.app/start/prerequisites/>.

## Chạy giao diện trên trình duyệt

```bash
npm install
npm run dev
```

Mở <http://localhost:1420>. Chế độ trình duyệt không có quyền nhập media vào vùng dữ liệu desktop.

## Chạy ứng dụng desktop

```bash
npm install
npm run tauri:dev
```

Trong ứng dụng, mở **Thư viện media** → **Nhập media**. File được sao chép vào thư mục dữ liệu riêng của FlowPost AI, vì vậy việc xóa hoặc di chuyển file gốc không ảnh hưởng đến kho media.

## Đóng gói bộ cài

```bash
npm run tauri:build
```

Bộ cài được tạo trong `src-tauri/target/release/bundle/`. Định dạng phụ thuộc hệ điều hành build: MSI/NSIS trên Windows, DMG trên macOS và AppImage/deb/rpm trên Linux.

> Tauri không cross-compile bộ cài desktop theo cách thông thường. Hãy build bản Windows trên Windows, bản macOS trên macOS và bản Linux trên Linux.

## Lấy Facebook Token (5 quyền bắt buộc)

FlowPost AI dùng token do bạn tự tạo và dán vào ứng dụng. Token cần có đúng 5 quyền sau:

- `pages_show_list` → `GET /me/accounts` liệt kê Trang (`facebook.rs:list_facebook_pages`)
- `pages_manage_posts` → `POST /{page_id}/feed` và `POST /{page_id}/photos` đăng bài (`facebook.rs:publish_content`)
- `pages_read_engagement` → `likes.summary(true)` và `comments.summary(true)` trong `facebook.rs:list_page_posts`
- `pages_read_user_content` → `message`/`created_time` trong `facebook.rs:list_page_posts`
- `read_insights` → `post_impressions`, `post_impressions_unique`, `post_engaged_users`, `post_clicks`, `post_reactions_by_type_total`, `post_video_views` (6 metric cho tooltip 2 cột, `facebook.rs:get_post_insights`)

1. Mở https://developers.facebook.com/tools/explorer/ → chọn App của bạn → `User Token` → `Add a Permission` tick 5 scopes trên → `Generate Access Token` → Copy. Token tạo ở Graph Explorer chỉ nên dùng để kiểm thử (xem mục production bên dưới).
2. Mở FlowPost AI → `Cài đặt` → `Kết nối` → dán vào `Nhập Facebook User/Page Access Token` → `Lưu` → `Kiểm tra` phải hiện `Token hợp lệ` (`GET /me`, `facebook.rs:validate_facebook_token`).
3. Bấm `Tải Trang` → backend gọi `GET /me/accounts?fields=id,name,access_token`, đổi User Token thành Page Token và lưu `fb_page_{id}`; frontend chỉ thấy `id/name` (`facebook.rs:list_facebook_pages`).
4. Chọn `Trang mặc định cho Lịch`. Danh sách Trang và bài viết gần đây được tải khi mở tab Dashboard hoặc khi chuyển tab; insights 6 metric chỉ được tải khi bạn bấm `Làm mới` trong khung "Bài viết gần đây". Hover `TIẾP CẬN` hiện bảng 2 cột; nếu thiếu `read_insights` thì `reach` giữ `—`.

> Lưu ý lưu trữ: token được lưu trong keyring của hệ điều hành. Từ giai đoạn bảo mật, fallback plaintext chỉ được ghi khi keyring thất bại; giá trị cũ vẫn được đọc để migrate. Chi tiết và giới hạn: xem `SECURITY.md`.

### Thay đổi hành vi gần đây

- Dashboard không còn tự gọi Graph API mỗi 4 giây; danh sách Trang và bài viết gần đây được tải khi mở/đổi tab, còn insights chỉ được tải khi người dùng bấm Làm mới. Khi mở app lần đầu, danh sách Trang và bài viết có thể được tải hai lần (một lần khi mount, một lần khi danh sách style tải xong).
- Chi tiết: danh sách Trang + bài viết gần đây tải khi mở tab hoặc đổi tab; insights chỉ tải khi bấm `Làm mới`.
- Token trang được lưu lại chỉ khi giá trị thay đổi.
- Mọi request Graph dùng Authorization: Bearer; token không còn nằm trong URL, form hoặc multipart.
- Lỗi mạng và body lỗi không phải JSON được scrub URL/token trước khi trả về frontend; lỗi Graph dạng JSON trả nguyên `message` của Meta.
- Lệnh get_facebook_token đã bị xóa; frontend không còn API nào trả token thô.
- `.env` đã được thêm vào `.gitignore`; `page_id` được kiểm tra chỉ gồm chữ số.

> Tip: User Token ngắn hạn 1-2h nên đổi sang dài hạn 60 ngày: `GET /oauth/access_token?grant_type=fb_exchange_token&client_id={app_id}&client_secret={app_secret}&fb_exchange_token={short_token}` — chỉ cần khi bạn có `APP_ID/SECRET` (FlowPost AI không lưu, bạn tự đổi trên Graph Explorer). Khi hết hạn (`code 190`) chỉ cần tạo token mới, dán lại và `Tải Trang` lại, không cần `APP_ID/SECRET` trong app (tránh lộ secret trên desktop).

## Khi gặp lỗi token hoặc app bị hạn chế

- Mã 190 nghĩa là access token không hợp lệ, đã hết hạn hoặc đã bị thu hồi. Không tạo app Meta mới để khắc phục.
- Nếu app bị hạn chế hoặc vô hiệu hóa, hãy kháng nghị trong Business Support Home (Account Quality) của Meta.
- Phân biệt: token hết hạn thì tạo token mới và dán lại; app bị hạn chế/vô hiệu hóa thì kháng nghị, không tạo app mới.
- Token tạo từ Graph Explorer là token ngắn hạn, chỉ dùng để kiểm thử, không dùng cho production.
- Tạo nhiều app liên tiếp sau mỗi lần lỗi 190 là hành vi bị Meta coi là tránh vi phạm và có thể khiến tài khoản developer bị khoá tính năng tạo app.

Thêm vào đó:

- Ghi lại **bên ngoài repo** những App ID đã từng tạo (kèm mục đích và thời điểm), để không phải đoán khi làm việc với Meta. Không đưa App ID hay token vào repo.
- Không xóa app cũ khi đang có kháng nghị mở.
- Không dùng proxy, xoay IP hay giả User-Agent để lách giới hạn của Meta.

### Chuẩn bị cho production

`pages_manage_posts` cần App Review (Advanced Access) và, trong một số trường hợp, cần Business Verification. App để ở Development mode chỉ hoạt động với admin/tester của chính app, còn token dán tay từ Graph Explorer là công cụ debug chứ không phải mô hình xác thực cho production. Khuyến nghị: dùng **một** app production với quy trình App Review đầy đủ, thay vì tạo nhiều app dùng một lần rồi bỏ.

## Lưu trữ cục bộ

Rust backend tạo thư mục `media` bên trong app data directory của hệ điều hành và lưu `media-index.json` tại cùng vị trí. Mỗi file được đổi sang UUID khi sao chép để tránh trùng tên; tên gốc, loại file, dung lượng và thời điểm nhập vẫn được giữ trong index.

Token Facebook và API key OpenAI Compatible Provider được lưu trong keyring của hệ điều hành; fallback plaintext `secure-credentials.json` trong `app_data_dir` chỉ được ghi khi keyring thất bại (xem `SECURITY.md`). Không đưa token vào frontend, file index hay git. Lỗi mạng và body lỗi không phải JSON đã được scrub URL/token trước khi lưu, còn lỗi Graph dạng JSON chỉ giữ `message` của Meta (không chứa token) nên `schedule-index.json` không còn nhận token qua `lastError`; tuy nhiên file do phiên bản cũ ghi vẫn có thể còn token và cần được kiểm tra/quay vòng thủ công.
