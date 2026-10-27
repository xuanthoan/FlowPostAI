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

- `bundle.icon` trong `src-tauri/tauri.conf.json` khai báo bộ icon dùng khi đóng gói; trên Windows, `icons/icon.ico` là file được WiX/NSIS dùng để gắn icon cho bộ cài và ứng dụng.
- Các file icon trong `src-tauri/icons/` được lưu trong git, nên bản clone mới vẫn build được mà không cần tạo lại icon.
- Trên Windows, MSI nằm trong `src-tauri/target/release/bundle/msi/`, còn NSIS `*-setup.exe` nằm trong `src-tauri/target/release/bundle/nsis/`. Nếu xóa `bundle.icon`, bước đóng gói MSI sẽ lỗi `Couldn't find a .ico icon`.

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
4. Chọn `Trang mặc định cho Lịch`. Danh sách Trang + 5 bài gần nhất + insights 6 metric (3 bài đầu) **chỉ được tải khi bạn bấm `Tải báo cáo`** trong khung "Bài viết gần đây". Chuyển tab, mở Dashboard hoặc quay lại cửa sổ không gọi Graph. Hover `TIẾP CẬN` hiện bảng 2 cột; nếu thiếu `read_insights` thì `reach` giữ `—`.

> Lưu ý lưu trữ: bản build này chưa bật backend keyring của hệ điều hành, nên secret được lưu trong `secure-credentials.json` (plaintext) tại `app_data_dir`. Cơ chế ghi keyring vẫn tồn tại và chỉ xóa bản fallback sau khi **đọc lại được** giá trị vừa ghi; giá trị cũ vẫn được đọc để migrate. Chi tiết và giới hạn: xem `SECURITY.md`.

### Thay đổi hành vi gần đây

- Dashboard không gọi Graph khi mở/đổi tab; mọi request báo cáo chỉ chạy khi bấm Tải báo cáo.
- `Đăng bài` là trang riêng trong sidebar, tách khỏi dashboard `Tổng quan`: danh sách Trang chỉ được tải khi bấm `Tải danh sách Trang` (không gọi Graph khi mở trang hay chuyển tab), nội dung lấy từ `Kho nội dung` là dữ liệu cục bộ đã có trong ứng dụng, và mỗi bài đính kèm tối đa 10 ảnh. Nút trong `Cài đặt` → `Kết nối` đổi tên thành `Đăng ngay` và mở modal dùng chung form đăng bài.
- Trang `Đăng bài` chia làm 2 cột: bên trái là khung soạn nội dung lớn, bên phải là khung media luôn hiển thị (tải `list_media` cục bộ khi mở trang, nút `Làm mới` để tải lại thư viện). Modal trong `Cài đặt` vẫn giữ bố cục 1 cột ~440px và chỉ tải media khi bấm `Chọn ảnh`.
- Chi tiết: nút Tải báo cáo tải danh sách Trang + bài viết gần đây + insights; dữ liệu giữ trong bộ nhớ đến khi tải lại. Số liệu "Bài viết tháng này"/"Đã đăng thành công" vẫn lấy từ dữ liệu cục bộ, không gọi Graph.
- Token trang được lưu lại chỉ khi giá trị thay đổi.
- Mọi request Graph dùng Authorization: Bearer; token không còn nằm trong URL, form hoặc multipart.
- Lỗi mạng và body lỗi không phải JSON được scrub URL/token trước khi trả về frontend; lỗi Graph dạng JSON trả nguyên `message` của Meta.
- Lệnh get_facebook_token đã bị xóa; frontend không còn API nào trả token thô.
- `.env` đã được thêm vào `.gitignore`; `page_id` được kiểm tra chỉ gồm chữ số.
- Đăng nhiều ảnh: tối đa 10 ảnh/bài. Với 1 ảnh, ứng dụng gọi POST /{page_id}/photos như trước. Với 2–10 ảnh, ứng dụng tải tuần tự từng ảnh bằng POST /{page_id}/photos với published=false, rồi tạo bài bằng POST /{page_id}/feed với attached_media[i]={"media_fbid":"..."}.
- Nếu một ảnh tải lên thất bại, bài viết không được tạo; các ảnh đã tải lên ở trạng thái chưa công bố và không được tự động xoá.
- Lịch đăng dùng toàn bộ ảnh hợp lệ của nội dung (tối đa 10). Ảnh/video thiếu tệp, quá 10MB hoặc sai định dạng bị bỏ qua; nếu không còn ảnh hợp lệ, bài chỉ có chữ.
- Ứng dụng không tự chống đăng trùng: nếu thao tác đăng được lặp lại (ví dụ mất phản hồi sau khi bài đã được tạo), có thể xuất hiện bài trùng; hãy kiểm tra Page trước khi đăng lại.

> Tip: User Token ngắn hạn 1-2h nên đổi sang dài hạn 60 ngày: `GET /oauth/access_token?grant_type=fb_exchange_token&client_id={app_id}&client_secret={app_secret}&fb_exchange_token={short_token}` — chỉ cần khi bạn có `APP_ID/SECRET` (FlowPost AI không lưu, bạn tự đổi trên Graph Explorer). Khi hết hạn (`code 190`) chỉ cần tạo token mới, dán lại và `Tải Trang` lại, không cần `APP_ID/SECRET` trong app (tránh lộ secret trên desktop).

## Báo cáo hiệu suất và xuất CSV/PDF

- Mở sidebar `Báo cáo`, chọn `Trang` và khoảng thời gian (`7 ngày qua`, `30 ngày qua`, `Tháng này` hoặc `Tùy chọn`), rồi bấm `Tải báo cáo`. Backend đọc bài viết bằng `GET /{page_id}/posts` với `since`/`until` và Page token đã lưu.
- `Tháng này` lấy từ 00:00 ngày 1 của tháng hiện tại đến thời điểm hiện tại theo giờ địa phương (month-to-date). `Tùy chọn` nhận `Từ ngày`/`Đến ngày` theo lịch địa phương và bao gồm cả hai đầu; tối đa 366 ngày và không cho chọn ngày trong tương lai. Hai khoảng này dùng lệnh `get_page_report_range`.
- Trang hiển thị `Tổng số bài`, `Tổng tương tác`, `Cảm xúc`, `Bình luận / Chia sẻ`. Công thức: **Tương tác = cảm xúc + bình luận + chia sẻ**, lấy từ `likes.summary.total_count`, `comments.summary.total_count` và `shares.count` của các bài đã tải. Số này khác với insights ở Dashboard (impressions/engaged/clicks) và chỉ là số liệu công khai.
- Giới hạn báo cáo: tối đa 100 bài/request và 20 trang (2.000 bài). Nếu còn cursor ở trang cuối, UI/CSV ghi cảnh báo đã đạt giới hạn; nếu một bài thiếu summary cảm xúc/bình luận, tổng được tính với 0 và ghi cảnh báo `partial` để không hiểu sai là số thật đầy đủ.
- `Xuất CSV` mở hộp thoại lưu tệp native, ghi UTF-8 có BOM, tên dạng `bao-cao-{slug}-{days}ngay-{YYYY-MM-DD}.csv` cho 7/30 ngày, `bao-cao-{slug}-thang-nay-{YYYY-MM-DD}.csv` cho `Tháng này`, và `bao-cao-{slug}-tu-{from}-den-{to}-{YYYY-MM-DD}.csv` cho `Tùy chọn`; trung hòa công thức bảng tính ở đầu ô. `Xuất PDF` gọi hộp thoại in của hệ thống; chọn **Save as PDF** để lưu. Hai nút này chỉ xuất báo cáo đã tải và bị tắt khi bộ lọc thay đổi.
- Quy tắc on-demand: mở trang, chuyển tab, đổi Trang/khoảng thời gian (kể cả đổi `Từ ngày`/`Đến ngày` của `Tùy chọn`) hoặc xuất CSV/PDF **không gọi Graph**. Chỉ `Tải báo cáo` (và `Tải lại danh sách Trang` để làm mới danh sách Trang) mới gọi Facebook. Ngày `Tùy chọn` không hợp lệ được chặn ngay trên UI, trước khi tải danh sách Trang.
- Quyền cần cho báo cáo: `pages_show_list` (danh sách Trang), `pages_read_engagement` và `pages_read_user_content` (bài viết + summary cảm xúc/bình luận/chia sẻ). Không cần `read_insights` cho trang báo cáo này.

## Tạo ảnh minh hoạ bằng AI

- Trang `AI Content` chia 2 cột: cột trái để tạo nội dung (chủ đề, prompt, style/model, tiêu đề, nội dung) và cột phải `Ảnh minh hoạ AI` (hộp hướng dẫn, phong cách ảnh, provider/model, nút `Tạo ảnh`, ảnh xem trước và media). Ảnh là tuỳ chọn: không bật gì thì trang chỉ tạo bài viết. Footer `Xóa form`/`Lưu vào kho` nằm dưới cả 2 cột.
- Nguồn tạo ảnh là **Tiêu đề + Nội dung** bài viết và `Phong cách ảnh` đang chọn; **bạn không cần viết prompt ảnh riêng**. Hộp hướng dẫn trong cột phải tóm tắt 2 cách dùng:
  - `Cách 1 — Tự động`: tick `Tạo ảnh minh hoạ bằng AI` (mặc định tắt) ở cột trái **trước khi** tạo nội dung; nút `Tạo nội dung` đổi thành `Tạo nội dung + Ảnh AI`, và một lần bấm tạo nội dung rồi tự tạo **1 ảnh** ngay sau đó.
  - `Cách 2 — Thủ công`: khi đã có Tiêu đề/Nội dung, bấm `Tạo ảnh` ở cột phải — dùng để tạo lại ảnh sau khi sửa bài, hoặc bấm `Thử tạo lại ảnh` khi lần trước bị lỗi. Nút `Tạo ảnh` chỉ bật khi có Tiêu đề hoặc Nội dung.
- `Phong cách ảnh` quyết định màu sắc, ánh sáng, không khí của ảnh — không phải nội dung bài viết. Đây là danh sách riêng (`image_styles.json`), có 3 style mặc định (`Điện ảnh`, `Tối giản`, `Minh hoạ phẳng`) và cho phép thêm/sửa/nhân bản/xóa/khôi phục mặc định.
- Chọn provider/model tạo ảnh: `Cài đặt` → tab `Model AI` → mục `Model tạo ảnh` (provider, model, kích thước, thư mục lưu). Chọn provider/model viết content ở cùng tab, mục `Model viết content`; AI Content vẫn có thể đổi model cho từng bài.
- Ứng dụng gọi `POST {base}/images/generations` theo chuẩn OpenAI-compatible với `n=1`, `response_format=b64_json`; nếu provider trả `url`, backend tự tải ảnh về (không gửi API key tới host tải ảnh).
- Ảnh luôn được lưu vào thư viện media của ứng dụng (`media/`) và được gắn vào bài viết để xem trước/đăng kèm. Nếu bạn chọn `Thư mục lưu ảnh`, ứng dụng ghi thêm một bản sao vào thư mục đó; lỗi ghi bản sao chỉ là cảnh báo, ảnh trong thư viện vẫn dùng được.
- Giới hạn: 1 ảnh mỗi lần tạo; ảnh phải là PNG/JPEG/WEBP/GIF và không quá 10MB; kích thước hỗ trợ `1024x1024`, `1024x1536`, `1536x1024`.
- Nếu tạo ảnh lỗi, bài viết đã tạo vẫn giữ nguyên; bấm `Thử tạo lại ảnh`. Ảnh không được tạo tự động khi bạn chỉ lưu bài viết.
- Cấu hình ảnh được lưu trong `image-config.json` (provider/model/style/thư mục/kích thước).

### Kho nội dung

- Mỗi dòng trong `Kho nội dung` có nút `Tạo ảnh AI` (bài chưa có ảnh) hoặc `Đổi ảnh AI` (bài đã có ảnh). Ảnh được tạo từ **Tiêu đề + Nội dung** của bài bằng `generate_image`, dùng style/provider/model/kích thước/thư mục lưu trong `Cài đặt` → `Model AI` (mục `Model tạo ảnh`). Không cần prompt ảnh riêng, nhưng bài phải có prompt nội dung vì `update_content` yêu cầu.
- Bài chưa có ảnh: bấm `Tạo ảnh AI` là tạo ngay và gắn đúng 1 ảnh mới (`mediaIds` = `[ảnh mới]`).
- Bài đã có ảnh: bấm `Đổi ảnh AI` mở hộp xác nhận **trước khi** gọi AI — `Thêm ảnh` nối ảnh mới vào sau các ảnh cũ (giữ thứ tự), `Thay ảnh` thay toàn bộ bằng ảnh mới; `Hủy`, phím `Esc` hoặc bấm nền tối đều đóng mà không tạo ảnh. Mỗi bài tối đa 10 ảnh; khi đã đủ 10, `Thêm ảnh` báo lỗi và bạn phải chọn `Thay ảnh`.
- Ảnh luôn được lưu vào `Thư viện media` và gắn vào bài; số `media` trên dòng cập nhật ngay sau khi xong (không cần tải lại trang). Trong lúc tạo, chỉ dòng đang chạy hiện `Đang tạo ảnh...` và mọi nút ảnh bị tắt.
- Nếu `generate_image` lỗi, bài viết giữ nguyên (không gọi `update_content`) và nút bật lại để thử tiếp. Nếu ảnh tạo xong nhưng `update_content` lỗi, bài viết vẫn giữ nguyên còn tệp ảnh đã tạo có thể còn trong `Thư viện media` (ứng dụng không tự xóa).
- Các thao tác này chỉ gọi IPC cục bộ: không gọi Graph/Facebook và không dùng `save_content`.

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

Token Facebook và API key OpenAI Compatible Provider được lưu trong `secure-credentials.json` (plaintext) tại `app_data_dir`; bản fallback chỉ bị xóa sau khi keyring ghi và đọc lại được giá trị vừa ghi (xem `SECURITY.md`). Không đưa token vào frontend, file index hay git. Lỗi mạng và body lỗi không phải JSON đã được scrub URL/token trước khi lưu, còn lỗi Graph dạng JSON chỉ giữ `message` của Meta (không chứa token) nên `schedule-index.json` không còn nhận token qua `lastError`; tuy nhiên file do phiên bản cũ ghi vẫn có thể còn token và cần được kiểm tra/quay vòng thủ công.
