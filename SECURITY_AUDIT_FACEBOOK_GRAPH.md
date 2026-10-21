# Kiểm toán bảo mật: tính năng đăng bài lên Facebook qua Graph API

> **Cập nhật 2026-10-21 — đã khắc phục.** Tài liệu này là ảnh chụp trạng thái TRƯỚC khi sửa. Toàn bộ M1–M7 và S1–S8 đã được xử lý theo 4 phase: Phase 1 (giảm tải Graph, bỏ polling 4s, insights thủ công), Phase 2 (sửa README/SECURITY), Phase 3 (Publishing state, cap/tick, delay, dedup, phân loại lỗi + backoff), Phase 4 (Bearer auth, scrub lỗi, fallback chỉ ghi khi keyring lỗi, bỏ get_facebook_token, validate page_id, .env vào .gitignore). Các mô tả bên dưới phản ánh code TRƯỚC remediation; đối chiếu `SECURITY.md` để xem hiện trạng.

**Phạm vi:** `src-tauri/src/facebook.rs`, `src-tauri/src/security.rs`, `src-tauri/src/schedule.rs`,
`src-tauri/src/auto_schedule.rs`, `src-tauri/src/lib.rs`, `src/App.jsx`, `README.md`, `SECURITY.md`, `.gitignore`
**Nhánh:** `dev` · **Phiên bản:** 0.3.0 · **Ngày audit:** 2026-10-21 · **Loại:** Phase 0 (chỉ đọc, không sửa mã nguồn)

### Ghi chú bắt buộc về cách trích dẫn (đọc trước)

> Mọi finding trong tài liệu này được neo theo **`file:symbol`** (tên hàm/hằng/component/struct),
> **không** neo theo số dòng. Số dòng chỉ mang tính tham khảo và sẽ trôi sau mỗi lần sửa code.
> Điều này không phải giả thuyết suông: `README.md` hiện đã trích `facebook.rs:128`, `:220`, `:327` trong khi hàm
> tương ứng nằm ở `list_facebook_pages`, `publish_content`, `get_post_insights` — lệch 9–17 dòng;
> `SECURITY.md` trích `facebook.rs:154` cho chỗ cache Page token, thực tế ở `list_facebook_pages` dòng 163.
> Vì vậy khi review, hãy `grep` theo **tên symbol**, đừng theo số dòng.

---

## 1. TL;DR

- **Không thể chứng minh nguyên nhân Meta khoá tính năng tạo app chỉ từ mã nguồn.** Lý do chính thức chỉ tồn tại
  trong thông báo của Meta (Business Support Home / Account Quality). Mọi kết luận dưới đây là **giả thuyết có
  xếp hạng độ tin cậy**, không phải kết luận thay Meta.
- **Giả thuyết mạnh nhất (Cao):** sau khi gặp `code 190`, tài liệu của chính dự án hướng dẫn người dùng
  **"tạo lại ở app khác"** (`README.md` bước 2). Tạo app mới liên tục sau khi app cũ bị xoá/bị khoá là hành vi
  Meta nhìn thấy rất rõ ở cấp tài khoản developer, và là con đường ngắn nhất tới việc khoá quyền tạo app.
- **Giả thuyết thứ hai (Trung bình):** tải request bất thường. Dashboard gọi Graph **5 request mỗi 4 giây**
  (≈1,25 req/s, ≈108.000 req/ngày nếu app mở 24/7), cộng với scheduler bật mặc định, không delay/jitter/cap và
  retry mù mọi loại lỗi — kể cả lỗi không được phép retry (`190`) và lỗi rate limit (`4`, `32`, `613`, `368`, `429`).
- **Giả thuyết thứ ba (Thấp–Trung bình):** token lộ. Token nằm trong query string, trong thông điệp lỗi
  (kèm URL), trong file plaintext `secure-credentials.json`, và có thể bị render ra UI qua `lastError`.
- **Không liên quan trực tiếp tới việc khoá app nhưng vẫn là lỗi bảo mật thật:** 8 mục SHOULD-FIX bên dưới.
- **Việc cần làm ngay (15 phút, không build):** dừng app, tắt auto-start, **không tạo thêm Meta app nào**,
  ghi lại toàn bộ App ID + ngày tạo, mở Business Support Home để lấy nguyên văn thông báo và appeal tại đó.

---

## 2. Bối cảnh & giới hạn bằng chứng

### 2.1. Đã xác minh trong phiên audit này (đọc trực tiếp file thật)

| Nội dung | Bằng chứng |
|---|---|
| Base URL Graph cố định `https://graph.facebook.com/v19.0` | `facebook.rs:graph_base` |
| Token đi trong query string ở 4 lời gọi GET | `facebook.rs:validate_facebook_token`, `list_facebook_pages`, `list_page_posts`, `get_post_insights` |
| Token đi trong multipart/form body ở 2 lời gọi POST | `facebook.rs:publish_content` (nhánh `/photos`, nhánh `/feed`) |
| Thông điệp lỗi mạng chèn nguyên `reqwest::Error` (Display có URL đầy đủ) | 6 chỗ `.map_err(... "Không thể kết nối Graph API: {}", e)` trong `facebook.rs` |
| `map_graph_error` chỉ xử lý `190`/HTTP 401 và `100`, không xử lý `4/32/613/368/429` | `facebook.rs:map_graph_error` |
| `publish_content` chỉ validate `page_id` khác rỗng, rồi nội suy thẳng vào path URL | `facebook.rs:publish_content` |
| `set_secret_hybrid` **luôn** ghi plaintext fallback khi có AppHandle, kể cả khi keyring thành công | `security.rs:set_secret_hybrid` (comment "we still write fallback as backup") |
| Fallback path = `app_data_dir()/secure-credentials.json` | `security.rs:fallback_path` |
| Page token lưu dưới key `fb_page_{id}` và ghi ngay trong `list_facebook_pages` | `facebook.rs:page_token_key`, `facebook.rs:list_facebook_pages` |
| `get_facebook_token` trả token thô ra webview và có trong `invoke_handler` | `security.rs:get_facebook_token`, `lib.rs:run().invoke_handler` |
| Frontend **không** gọi `get_facebook_token` | quét `src/` + `e2e/` → 0 lời gọi |
| `SchedulerConfig::default()` = `interval_seconds: 30`, `enabled: true` | `schedule.rs:SchedulerConfig::default` |
| Scheduler + auto-scheduler tự start trong `.setup()` mỗi lần mở app | `lib.rs:run()` (spawn `start_scheduler` + `start_auto_scheduler`) |
| `process_due_schedules` là một vòng `for` duy nhất, không sleep/jitter/cap, ghi index **một lần** sau vòng lặp | `schedule.rs:process_due_schedules` |
| Retry mù: `retry_count += 1` cho mọi `Err`, tối đa 3, delay 5/10/30 phút | `schedule.rs:process_due_schedules`, `schedule.rs:retry_delay_minutes` |
| `ScheduleStatus` chỉ có `Scheduled/Published/Failed/Cancelled` (không có trạng thái đang gửi) | `schedule.rs:ScheduleStatus` |
| Đúng 4 vòng `setInterval(..., 4000)` trong `src/App.jsx` | dòng 412, 1405, 1548, 1688 |
| Vòng lặp dashboard chạy suốt phiên, không phụ thuộc tab đang mở | `App.jsx:App` effect deps `[desktop, active, styleList]`, component luôn mounted |
| Mỗi chu kỳ dashboard = 5 lời gọi Graph | `App.jsx:App` (`list_facebook_pages` + `list_page_posts` + 3× `get_post_insights`) |
| Lỗi insights bị nuốt hoàn toàn | `App.jsx:App` (`catch { return null }`, `catch { /* giữ '—' */ }`) |
| `lastError` của lịch được render ra UI | `App.jsx:SchedulePage`, `App.jsx` (bảng lịch AI content) |
| `README.md` hướng dẫn tạo app mới sau `code 190` | `README.md` bước 2 ("nếu `code 190` → app đã bị xóa, tạo lại ở app khác") |
| `README.md` nói token "không vào JSON" | `README.md` bước 2 — **sai** so với `security.rs:set_secret_hybrid` |
| `README.md` nói dashboard gọi Graph sau 4s | `README.md` bước 4 |
| `SECURITY.md` mô tả plaintext fallback như hành vi chỉ của Linux | `SECURITY.md` mục "Lưu trữ secret" (dòng Linux) |
| `.gitignore` gốc không có `.env`; chỉ `e2e/.gitignore` bỏ qua `e2e/.env` | `.gitignore`, `e2e/.gitignore`, `git check-ignore -v .env` → exit 1 |
| Không có `app_secret` / `client_secret` / `appsecret_proof` / OAuth flow nào trong `src/` | quét `src/`, `src-tauri/src/` → 0 kết quả |
| Không có HTTP test cho `/feed`, `/photos`, `/me/accounts`; `wiremock` chỉ dùng ở `ai.rs` | `src-tauri/Cargo.toml`, `ai.rs` |
| Không có commit nào chứa mẫu token (`git log --all -S"EAA"`) | output rỗng |
| Làm việc tree sạch, không có token-like string ngoài `node_modules/target/.git/dist` | `git status --porcelain` rỗng + quét mẫu token → 0 |

### 2.2. Trạng thái runtime trên máy audit (chỉ đếm, không in giá trị)

| Kiểm tra | Kết quả |
|---|---|
| `%APPDATA%\vn.flowpost.desktop\secure-credentials.json` tồn tại | **Có** (157 bytes, LastWriteTime 2026-10-20 21:25:30) |
| Tên key trong file đó | chỉ `ai_provider_omniroute_key`, `ai_provider_gemini_key` — **không có** `facebook_token` / `fb_page_*` |
| Số lần xuất hiện chuỗi `access_token` trong 8 file JSON runtime | **0** ở tất cả |
| Số lần xuất hiện mẫu token (`EAA` + 20 ký tự) | **0** ở tất cả |
| `schedule-index.json` | **chưa tồn tại** (thư mục `schedule` rỗng) |
| `schedule-config.json` | `"enabled": true`, `"intervalSeconds": 30`, `"selectedPageId": null` |
| Credential Manager có entry `vn.flowpost.desktop` | **Không** (`cmdkey /list` không khớp) |

→ Trên **máy này** chưa từng lưu Facebook token, nên chưa có sự cố lộ token cần báo động.
→ Trên **máy đã dùng tính năng đăng bài**, các bảng trên phải được chạy lại (xem M7) vì cơ chế ghi plaintext
và cơ chế ghi `last_error` là có thật trong code.

### 2.3. Chưa xác minh (không thể xác minh bằng code)

- Nguyên văn thông báo khoá của Meta, ngày giờ, App ID bị ảnh hưởng, loại hạn chế (app creation / developer account).
- Số app đã tạo, số Page/định danh tài khoản đã dùng.
- Trạng thái App Review, Business Verification, "Require App Secret", chế độ Development/Live của app.
- Thời lượng app thực sự chạy (để tính tổng request thật) — **dự án không có audit log request Graph** (xem S7).
- Giá trị bên trong `secure-credentials.json` (đã **không đọc, không in** theo yêu cầu).
- Việc `e2e/.env` cục bộ có chứa token thật hay không (chỉ xác minh file **tồn tại**, không đọc nội dung).

---

## 3. Ba giả thuyết

| # | Giả thuyết | Cơ chế dẫn tới khoá | Bằng chứng trong repo | Độ tin cậy | Cách xác nhận |
|---|---|---|---|---|---|
| **H1** | Tạo lại app mới sau khi gặp `code 190` | Meta coi việc liên tục tạo app mới để né app bị xoá/hạn chế là hành vi tránh vi phạm ở cấp tài khoản developer → hạn chế quyền tạo app | `README.md` bước 2 hướng dẫn nguyên văn "tạo lại ở app khác"; `README.md` phần "Lấy Facebook Token" chỉ nói về Graph Explorer, không có App Review; `facebook.rs:validate_facebook_token` + `map_graph_error` biến mọi `190` thành gợi ý "Tải Trang lại" | **Cao** | Đếm số app trong `developers.facebook.com/apps` + ngày tạo; đối chiếu mốc thời gian bị khoá; đọc nguyên văn thông báo trong Business Support Home |
| **H2** | Tải request bất thường + burst + retry mù | Mẫu truy cập không giống người dùng thật: polling cố định 4s, nhiều request đồng thời, scheduler 30s, retry không backoff đúng loại lỗi → chạm platform rate limit / bị đánh dấu automation, leo thang thành hạn chế tài khoản | `App.jsx:App` (5 Graph call/4s), `App.jsx:SettingsStatus` (2 call/4s), `schedule.rs:process_due_schedules` (không delay/jitter/cap), `schedule.rs:retry_delay_minutes`, `map_graph_error` bỏ qua `4/32/613/368/429`, `README.md` bước 4 xác nhận hành vi 4s | **Trung bình** | Đối chiếu mốc bị khoá với thời gian app chạy; xem cảnh báo rate limit trong App Dashboard; dựng lại lịch sử request từ log mạng/hệ điều hành (repo không có audit log) |
| **H3** | Token bị lộ và bị dùng ở nơi khác | Token trong query string + trong message lỗi + trong file plaintext + có thể hiện trên UI → lộ qua log, ảnh chụp màn hình, backup, phần mềm diệt virus/EDR, hoặc người khác dùng chung máy → request lạ từ IP/danh tính khác | `facebook.rs` (4 URL GET chứa `access_token`), `facebook.rs:publish_content` (form field), `security.rs:set_secret_hybrid` (luôn ghi plaintext), `App.jsx:SchedulePage` (render `lastError`), `security.rs:get_facebook_token` | **Thấp–Trung bình** | Thu hồi token rồi xem còn request lạ không; kiểm tra phiên đăng nhập/lịch sử hoạt động của tài khoản; kiểm tra quyền file `secure-credentials.json`; rà backup/EDR log (không in giá trị token) |

> Ba giả thuyết **không loại trừ nhau**. H1 và H2 có thể cùng đúng và cộng hưởng: app bị hạn chế → `190` →
> làm theo README tạo app mới → Meta thấy chuỗi hành vi → khoá quyền tạo app.

---

## 4. Findings

Mức độ: **Cao** = có thể trực tiếp gây hậu quả nghiêm trọng/sự cố; **Trung bình** = cần điều kiện nhất định;
**Thấp** = rủi ro phụ/defense-in-depth. Cột **Phase** = phase khắc phục đề xuất (xem mục 7).

### 4.1. MUST-FIX

| ID | Vấn đề | Bằng chứng (file:symbol) | Mức | Phase |
|---|---|---|---|---|
| **M1** | Tài liệu hướng dẫn **tạo app mới khi gặp `code 190`**. Đây là hướng dẫn nguy hiểm nhất trong repo: biến một lỗi token thành hành vi tạo app hàng loạt, đúng mẫu hành vi Meta hạn chế ở cấp tài khoản developer | `README.md` bước 2: "nếu `code 190` → app đã bị xóa, tạo lại ở app khác"; `README.md` phần "Lấy Facebook Token (5 quyền bắt buộc)" | **Cao** | 0 + 2 |
| **M2** | **4 vòng polling 4 giây**, trong đó vòng dashboard gọi Graph **5 lần/4s** (≈1,25 req/s, ≈108.000 req/ngày nếu mở 24/7), chạy **suốt phiên** bất kể tab đang mở, không pause khi cửa sổ ẩn, không cache, và mỗi chu kỳ còn **ghi lại page token** vào keyring + file | `App.jsx:App` (effect `[desktop, active, styleList]`, `setInterval(load, 4000)`), `App.jsx:SettingsStatus`, `App.jsx:SchedulePage`, `App.jsx:DashboardSchedulePanel`, `facebook.rs:list_facebook_pages` (gọi `set_secret_hybrid` mỗi lần) | **Cao** | 1 |
| **M3** | **Scheduler bật mặc định + tự start mỗi lần mở app + không delay/jitter/cap**: `enabled: true`, interval 30s, chạy không cần người dùng bật; mỗi tick xử lý **toàn bộ** item đến hạn trong một vòng `for` không nghỉ | `schedule.rs:SchedulerConfig::default`, `schedule.rs:start_scheduler`, `schedule.rs:process_due_schedules`, `lib.rs:run()` (spawn trong `.setup()`) | **Cao** | 3 |
| **M4** | **Retry mù**: mọi `Err` đều `retry_count += 1` và hẹn lại 5/10/30 phút — kể cả `190` (token chết, retry vô ích và tạo thêm request lỗi) và `4/32/613/368/429` (rate limit/policy). `map_graph_error` không phân loại các mã này nên tầng trên không thể quyết định đúng | `schedule.rs:process_due_schedules`, `schedule.rs:retry_delay_minutes`, `facebook.rs:map_graph_error` (chỉ nhánh `190`/401 và `100`) | **Cao** | 3 |
| **M5** | **Không có trạng thái "đang gửi" + không dedup → đăng trùng**: status chỉ chuyển `Published` **sau khi** POST trả về; index chỉ ghi **một lần sau cả vòng lặp**. App crash/tắt giữa lúc POST thành công nhưng chưa ghi index → lần chạy sau đăng lại cùng nội dung lên cùng Page. Retry mù (M4) làm việc này dễ xảy ra hơn | `schedule.rs:ScheduleStatus` (không có `Publishing`), `schedule.rs:process_due_schedules` (gán `Published` sau `await`, `write_schedule_index` cuối hàm), `ScheduleItem` không có trường chống trùng | **Trung bình–Cao** | 3 |
| **M6** | **Thiếu containment vận hành**: không có cơ chế nào để "dừng mọi traffic Graph" ngoài việc tắt app; không có tài liệu nào cấm tạo app mới; không có nơi lưu vết các app đã tạo. Khi bị hạn chế, phản xạ tự nhiên (theo README) là tạo app mới → làm tình hình xấu đi | `README.md`, `SECURITY.md`, `lib.rs:run()` (auto-start), `App.jsx:App` (polling vô điều kiện) | **Cao** | 0 |
| **M7** | **Rủi ro token tồn tại trong file state**: `last_error` được lưu vào `schedule-index.json`, và message lỗi mạng chứa URL có `access_token`. Trên máy đã chạy tính năng này, file state có thể đang chứa token dạng plaintext. Đã kiểm tra máy audit: **0 lần** xuất hiện `access_token` (chưa từng có lịch) | `facebook.rs` (6 chỗ map_err chèn `reqwest::Error`), `security.rs:write_fallback_map`, `schedule.rs:process_due_schedules` (`last_error = Some(e)`) | **Cao** | 0 |

### 4.2. SHOULD-FIX

| ID | Vấn đề | Bằng chứng (file:symbol) | Mức | Phase |
|---|---|---|---|---|
| **S1** | **Token trong query string** ở 4 lời gọi GET (`/me`, `/me/accounts`, `/posts`, `/insights`) và trong form/multipart body ở POST. URL đầy đủ đi vào access log của proxy/EDR, crash report, DevTools, ảnh chụp màn hình debug. Graph API hỗ trợ `Authorization: Bearer` — không có lý do dùng query string | `facebook.rs:validate_facebook_token`, `:list_facebook_pages`, `:list_page_posts`, `:get_post_insights`, `:publish_content` | **Trung bình** | 4 |
| **S2** | **Fallback plaintext luôn được ghi** kể cả khi keyring thành công (`set_secret_hybrid`), nên "kho bảo mật OS" thực chất là **2 bản sao**, trong đó 1 bản là JSON đọc được. `SECURITY.md` mô tả fallback như hành vi **chỉ của Linux** → sai. `README.md` nói token "không vào JSON" → sai. File JSON kế thừa ACL thư mục `app_data` (không phải DPAPI), nên mọi tiến trình chạy cùng user đều đọc được | `security.rs:set_secret_hybrid` (nhánh `if let Some(a) = app` luôn `set_fallback`), `security.rs:fallback_path`, `SECURITY.md` mục "Lưu trữ secret", `README.md` bước 2 | **Trung bình–Cao** | 4 |
| **S3** | **`get_facebook_token` trả token thô ra webview** và vẫn nằm trong `invoke_handler`. Hiện frontend không gọi (đã quét), nhưng đây là bề mặt tấn công thừa: XSS/ dependency độc trong renderer → đọc token trực tiếp, không cần qua Graph | `security.rs:get_facebook_token`, `lib.rs:run().invoke_handler` | **Trung bình** | 4 |
| **S4** | **`page_id` không được validate** (chỉ `trim().is_empty()`), rồi nội suy thẳng vào path: `/{page_id}/photos`, `/{page_id}/feed`. `page_id` chứa `/`, `?`, `#`, `..` có thể đổi path/query của request (path injection). Tác động bị giới hạn vì vẫn cần token hợp lệ, nhưng đây là input không đáng tin đi vào URL | `facebook.rs:publish_content` (validate + 2 chỗ `format!`), `facebook.rs:list_page_posts`, `facebook.rs:get_post_insights` | **Thấp–Trung bình** | 4 |
| **S5** | **Token có thể lộ ra UI**: `lastError` (chứa message lỗi mạng có URL kèm token) được render trực tiếp trong bảng lịch và bảng lịch AI content → hiện trên màn hình, dễ vào ảnh chụp màn hình/stream/screenshot gửi hỗ trợ | `App.jsx:SchedulePage` (render `s.lastError`), `App.jsx` (bảng `item.lastError`), `facebook.rs` (map_err chèn `reqwest::Error`) | **Trung bình–Cao** | 3 |
| **S6** | **Graph v19.0 cố định + không User-Agent + tạo `reqwest::Client` mới cho mỗi request**. v19.0 là bản cũ (mỗi version Graph có vòng đời giới hạn — cần đối chiếu changelog của Meta trước khi kết luận), client mới mỗi lần làm mất connection pool/TLS session reuse và không có UA nhận diện ứng dụng | `facebook.rs:graph_base`, 6 chỗ `Client::builder().build()` trong `facebook.rs` | **Thấp** | 4 |
| **S7** | **Không có audit log request Graph + nuốt lỗi insights**. Không thể dựng lại "app đã gọi gì, lúc nào, bao nhiêu lần" để tự bảo vệ khi appeal; lỗi insights bị `catch` im lặng nên token bị thu hồi cũng không hiện ra, người dùng chỉ thấy "—" | `App.jsx:App` (`catch { return null }`, `catch { /* giữ '—' */ }`), không tồn tại module logging nào trong `src-tauri/src/` | **Thấp–Trung bình** | 2 + 3 |
| **S8** | **`.env` không được gitignore ở root** (chỉ `e2e/.gitignore` bỏ qua `e2e/.env`). Developer đặt `.env` ở root (thói quen rất phổ biến) sẽ thấy nó trong `git status` và rất dễ commit | `.gitignore` (không có `.env`), `e2e/.gitignore`, `git check-ignore -v .env` → exit 1 (không bị ignore) | **Thấp** | 4 |

### 4.3. INFO-ONLY

| ID | Nội dung | Ghi chú |
|---|---|---|
| **I1** | **Không thể chứng minh nguyên nhân khoá từ code.** Code chỉ cho thấy *các đường dẫn rủi ro tồn tại*, không cho thấy Meta đã phản ứng với đường nào | Cần nguyên văn thông báo của Meta (mục 6, bước 5) |
| **I2** | **108.000 request/ngày là ước lượng tĩnh**, giả định: có token, app mở 24/7, dashboard luôn mounted, mỗi chu kỳ đủ 5 call thành công. Số thật phụ thuộc uptime và việc token có tồn tại (khi chưa có token, `list_facebook_pages` fail sớm trước khi gọi mạng) | Không dùng con số này làm bằng chứng thay log thật |
| **I3** | **KHÔNG thêm `app_secret`/`appsecret_proof` vào client desktop.** Nhúng app secret vào app phát hành cho người dùng = phát tán secret cho tất cả người dùng, tệ hơn nhiều so với rủi ro hiện tại. `appsecret_proof` chỉ đúng khi có backend giữ secret. **Đề xuất này bị loại bỏ rõ ràng** | Quét `src/` + `src-tauri/src/`: 0 lần xuất hiện `app_secret`/`client_secret`/`appsecret_proof` — hiện trạng là **đúng**, giữ nguyên |
| **I4** | **Graph version deprecation không phải nguyên nhân khoá tính năng tạo app.** Version cũ gây lỗi API (`2635`/version không tồn tại), không gây hạn chế ở cấp tài khoản developer | Sửa v19.0 vẫn cần (S6) nhưng không phải fix cho sự cố này |
| **I5** | **Lịch sử git và `e2e/.env` chưa kết luận được**: `git log --all -S"EAA"` rỗng (không có commit chứa mẫu token), nhưng `e2e/.env` **tồn tại cục bộ** và không được đọc trong phiên này. `SECURITY.md` ghi lịch sử từng bị viết lại bằng `git filter-repo`, nên "không thấy trong git" không đồng nghĩa "chưa từng tồn tại" | Cần người có quyền kiểm tra `e2e/.env` (không in giá trị) |
| **I6** | **Rate limit của Meta không phải ngưỡng cố định để "canh"**. Giới hạn phụ thuộc loại endpoint, số Page, lượng impression, và hành vi; con số cụ thể thay đổi theo thời gian. Mục tiêu đúng là **giảm tải và tôn trọng backoff**, không phải tìm ngưỡng để chạy sát | Mọi đề xuất "chạy dưới ngưỡng X req/s" đều bị loại bỏ |

---

## 5. "Điều có thể khiến bạn bị khoá" vs "Không liên quan đến khoá nhưng vẫn là lỗi bảo mật"

### 5.1. Có khả năng liên quan trực tiếp tới việc bị khoá (giả thuyết)

1. **Tạo app mới sau `code 190`** — hành vi được chính tài liệu dự án hướng dẫn (M1). Đây là mục có tín hiệu cao nhất.
2. **Vòng đời app không đúng chuẩn production**: app ở Development mode, token dán tay từ Graph Explorer, không có App Review / Business Verification, không có user thật cấp quyền (mục 8).
3. **Mẫu tải bất thường**: polling 4 giây (M2) + scheduler 30 giây bật mặc định (M3) + không delay/jitter/cap.
4. **Retry mù cả lỗi không retry được** (M4) — biến một lỗi thành chuỗi lỗi, gồm cả việc đập vào rate limit/policy block.
5. **Đăng trùng lặp** do crash giữa POST và ghi index (M5) — nội dung giống nhau lặp lại là tín hiệu spam.

### 5.2. Không liên quan đến việc khoá, nhưng vẫn là lỗi bảo mật (phải sửa)

- Token trong query string và trong form body (S1).
- Bản sao plaintext luôn được ghi, tài liệu mô tả sai (S2).
- `get_facebook_token` trả token thô ra webview (S3).
- `page_id` không validate (S4).
- Token có thể hiện trên UI qua `lastError` (S5).
- Graph cũ, không UA, client-per-request (S6).
- Không audit log, nuốt lỗi insights (S7).
- `.env` không được gitignore (S8).

### 5.3. Quan sát bổ sung (chưa lập ID, đã xác minh trong code)

- `assetProtocol.scope` trong `tauri.conf.json` là `["$APPDATA/**"]` — **bao trùm cả**
  `%APPDATA%\vn.flowpost.desktop\secure-credentials.json`. CSP hiện chỉ cho `asset:` ở `img-src`, nên chưa
  có đường đọc nội dung trực tiếp từ renderer, nhưng scope rộng hơn mức cần thiết (chỉ cần thư mục media).
  Đề xuất thu hẹp scope khi làm Phase 4.
- Việc polling gọi `list_facebook_pages` mỗi 4 giây khiến `set_secret_hybrid` **ghi keyring + JSON mỗi chu kỳ**
  cho từng Page — vừa tăng tải I/O, vừa là lý do nên "chỉ ghi khi token thay đổi" ở Phase 1.

---

## 6. Containment checklist (≤15 phút, không cần build)

> Mục tiêu: **ngừng mọi traffic Graph ngay**, bảo toàn bằng chứng, và **không làm tình hình xấu thêm**.
> Không cần sửa code, không cần cài lại app.

- [ ] **B1. Dừng app và xác nhận đã dừng.** Thoát FlowPost AI; mở Task Manager, kết thúc mọi tiến trình
      FlowPost AI còn sống. Kiểm tra không còn tiến trình nào trước khi sang bước sau (vì scheduler tự start
      ở `lib.rs:run()` mỗi lần mở app — mở lại app = bật lại traffic).
- [ ] **B2. Chặn auto-start trước khi mở app lần nữa.** Sao lưu rồi sửa
      `%APPDATA%\vn.flowpost.desktop\schedule-config.json` → `"enabled": false`
      (và `auto-schedule` config nếu có). Sao lưu `schedule-index.json` nếu tồn tại (**không mở/in nội dung**,
      vì có thể chứa token trong `lastError`).
- [ ] **B3. Không tạo bất kỳ Meta app / Page / token mới nào.** Không xoá app cũ (xoá = mất bằng chứng và
      có thể bị hiểu là che giấu). Không tạo tài khoản developer mới.
- [ ] **B4. Ghi lại toàn bộ app đã tạo:** App ID, tên, ngày tạo, mục đích, app nào từng bị lỗi `190`.
      Lấy từ `developers.facebook.com/apps`. Ghi ra file ngoài repo (đừng commit).
- [ ] **B5. Mở Business Support Home / Account Quality**, chụp/copy **nguyên văn** thông báo (đúng từng chữ,
      kèm ngày giờ, App ID, loại hạn chế) và **appeal tại đó**. Đây là kênh duy nhất có thẩm quyền —
      không có "mẹo" nào trong code thay thế được.
- [ ] **B6. Nếu nghi token đã lộ:** thu hồi token (revoke), gỡ Page role lạ, đổi mật khẩu Facebook, bật 2FA;
      sau đó xoá `facebook_token` + `fb_page_*` khỏi Credential Manager **và** khỏi
      `secure-credentials.json` (xoá file hoặc xoá đúng key — không in nội dung ra màn hình/chat).
- [ ] **B7. Chỉ nối lại traffic khi (a) Meta đã có quyết định, và (b) Phase 1 đã xong.** Không "chạy thử
      một bài cho biết" trong lúc chờ — mỗi lần chạy thử đều là bằng chứng mới chống lại appeal.

**Bị loại bỏ rõ ràng (không được làm):** xoay proxy, xoay IP, đổi/spoof User-Agent để né phát hiện,
"chạy dưới ngưỡng rate limit", tạo app mới, mượn app của người khác, dùng token của người khác.

---

## 7. Kế hoạch khắc phục theo phase

> Mỗi phase cần **approval riêng** trước khi triển khai. Phase 0 đã hoàn thành bằng tài liệu này.
> Không phase nào được phép gộp vào phase khác mà không xin approval.

### Phase 0 — Audit & containment (tài liệu này)

- Đầu ra: file này + checklist mục 6 đã chạy.
- **Exit criteria:** app đã dừng, auto-start đã tắt, đã ghi danh sách App ID, đã copy nguyên văn thông báo Meta,
  **không** tạo app mới, không có thay đổi mã nguồn.

### Phase 1 — Giảm tải Graph (ưu tiên cao nhất về kỹ thuật)

1. Polling 4 giây → **thủ công + tối thiểu 60 giây**; bỏ polling vô điều kiện ở `App.jsx:App`.
2. **Pause khi cửa sổ ẩn**: dùng `visibilitychange`/`focus` để dừng interval, resume khi hiện lại.
3. **Insights theo yêu cầu** (nút "Làm mới" hoặc hover mở tooltip), không fetch 3 bài mỗi chu kỳ.
4. `list_facebook_pages` **chỉ ghi token khi giá trị thay đổi** (so sánh trước khi `set_secret_hybrid`).
5. Chỉ chạy polling khi **đang ở tab liên quan** và **có token**.
- **Exit criteria:** số request Graph/ngày giảm ≥95% so với ước lượng 108k (đo bằng audit log thêm ở S7),
  không còn interval nào <60s, có đường tắt hoàn toàn (kill switch) cho traffic Graph.

### Phase 2 — Sửa tài liệu & minh bạch

1. `README.md`: xoá hướng dẫn "tạo lại ở app khác"; thay bằng quy trình appeal + "không tạo app mới";
   sửa câu "không vào JSON"; nêu rõ fallback plaintext tồn tại trên **mọi** nền tảng khi có AppHandle.
2. `SECURITY.md`: bỏ mô tả "Linux-only"; nêu đúng `set_secret_hybrid` luôn ghi fallback; sửa câu
   "frontend không bao giờ nhận giá trị thô" (vì `get_facebook_token` vẫn trả token thô cho tới Phase 4).
3. Thêm mục "Khi bị hạn chế tài khoản: KHÔNG tạo app mới" + checklist mục 6 vào tài liệu vận hành.
4. Thêm audit log tối thiểu (endpoint + mã lỗi + thời điểm, **không** log token/URL).
- **Exit criteria:** không còn câu nào trong repo hướng dẫn tạo app mới; không còn tuyên bố sai về nơi lưu token.

### Phase 3 — An toàn khi đăng bài

1. **Trạng thái `Publishing` được ghi xuống đĩa TRƯỚC khi POST**; ghi index **sau từng item**, không chỉ cuối vòng lặp.
2. **Per-tick cap** (ví dụ 1–2 item/tick) + **delay 2–5s + jitter** giữa các lần đăng.
3. **Dedup theo `content_id + page_id`** (kèm dấu "đã đăng" / `posted_at`) để crash không tạo bài trùng.
4. **Phân loại lỗi rõ ràng**:
   - `190` → `Failed`, **không retry**, hiện hướng dẫn "cấp lại token" (không hướng dẫn tạo app).
   - `4`, `32`, `613`, `368`, `429` → **backoff dài + dừng tick ngay**, không tăng `retry_count` như lỗi thường.
   - `100` → `Failed`, không retry (lỗi tham số).
   - Lỗi mạng → retry có cap như hiện tại nhưng có jitter.
5. Sanitize `last_error` trước khi lưu/ hiển thị (bỏ URL, bỏ query string) — đóng luôn S5/M7.
- **Exit criteria:** test cho 3 kịch bản (crash giữa POST, `190`, `429`) đều không sinh bài trùng và không retry sai;
  không có token trong `last_error`.

### Phase 4 — Vệ sinh secret

1. Chuyển toàn bộ Graph call sang header `Authorization: Bearer <token>`, bỏ `access_token` khỏi query/form.
2. **Scrub URL khỏi message lỗi** (`reqwest::Error` → chỉ lấy `is_timeout()`/`status()`/loại lỗi).
3. **Ngừng ghi fallback khi keyring OK**, nhưng **vẫn đọc fallback cũ** để migrate; thêm bước migrate rồi xoá.
4. **Bỏ `get_facebook_token` khỏi `invoke_handler`** (và khỏi code nếu không còn dùng).
5. **Validate `page_id` là số** (`is_ascii_digit`), từ chối trước khi tạo URL.
6. **`reqwest::Client` dùng chung** + `User-Agent` nhận diện app; nâng Graph version sau khi đối chiếu changelog.
7. Thêm `.env` vào `.gitignore` gốc.
8. Thu hẹp `assetProtocol.scope` về đúng thư mục media (mục 5.3).
- **Exit criteria:** 0 lần `access_token` xuất hiện trong URL/log/state; 0 command trả secret thô;
  `git check-ignore .env` trả về match; test vẫn xanh.

---

## 8. Bảng đối chiếu quyền / token

| Cách lấy token | Thời hạn | Dùng được cho production? | Ghi chú |
|---|---|---|---|
| User token dán tay từ **Graph Explorer** (cách `README.md` đang hướng dẫn) | Ngắn hạn (1–2 giờ) | **Không** | Đây là công cụ debug. Token do chính developer tạo trong phiên Explorer, không có luồng cấp quyền cho người dùng cuối, không có refresh, không audit được. Mọi app "production" dựa vào cách này đều phải tạo token lại liên tục — và đó chính là cái bẫy dẫn tới `190` rồi tạo app mới |
| User token dài hạn qua `fb_exchange_token` | ~60 ngày | Không khuyến nghị cho desktop | Cần `app_secret` ở client → **bị loại bỏ** (I3). Chỉ hợp lý nếu có backend giữ secret |
| Page token lấy từ `GET /me/accounts` (cách app đang dùng) | Kế thừa thời hạn của user token sinh ra nó | Chỉ khi user token đúng chuẩn | Page token **không** tự gia hạn; hết hạn cùng user token. App cache dưới `fb_page_{id}` và không có cơ chế phát hiện hết hạn trước khi POST |
| App ở **Development mode** | — | **Không** | Chỉ admin/developer/tester của app thao tác được; người dùng ngoài danh sách không dùng được. Đây là lý do nhiều app "chạy được vài lần" rồi dừng: phụ thuộc role cá nhân, không phải quyền của app |
| App ở **Live mode** + App Review | — | **Có** | `pages_manage_posts` (đăng bài) cần **Advanced Access = App Review**; một số quyền/kịch bản còn cần **Business Verification**. Cần đối chiếu yêu cầu hiện hành trong App Review submission của Meta |
| Người dùng cuối cấp quyền qua OAuth + backend đổi token | — | **Có** (kiến trúc đúng) | Secret nằm ở server; client chỉ giữ session. Đây là hướng đúng nếu muốn nhiều người dùng thật |

**Kết luận mục 8:** vấn đề gốc không phải "thiếu một mẹo Graph", mà là **mô hình cấp quyền sai**:
token dán tay từ Graph Explorer + app Development mode + không App Review. Cách đúng là **một app production,
App Review + Business Verification, luồng cấp quyền rõ ràng** — không phải tạo app mới mỗi lần token chết.

---

## 9. Câu hỏi còn mở

1. **Nguyên văn thông báo của Meta là gì, ngày giờ nào, và hạn chế ở cấp nào** (app creation / developer account / app cụ thể)?
2. **Bao nhiêu Page và bao nhiêu tài khoản Facebook** đã được dùng với app này?
3. **Trạng thái hiện tại của app:** Development hay Live? đã **App Review** chưa? đã **Business Verification** chưa?
   có bật **Require App Secret** không?
4. **App có chạy 24/7 không**, hay chỉ vài giờ mỗi lần? (dùng để kiểm chứng ước lượng 108k request/ngày)
5. **Đã tạo bao nhiêu app sau lần lỗi `190` đầu tiên**, và cách nhau bao lâu? (bằng chứng trực tiếp cho H1)
6. **Có hiện tượng nội dung bị đăng trùng** trên Page không? (bằng chứng cho M5)
7. **`e2e/.env` hoặc `secure-credentials.json` trên máy đã dùng có chứa token thật không?**
   (chỉ cần câu trả lời có/không — **không đọc, không in giá trị**; nếu có, coi là sự cố bảo mật và xử lý theo mục 6 bước B6)

---

## 10. Checklist xác minh citation

Mọi dòng dưới đây đã được **đọc lại file thật trong phiên audit này**. Cột "Khớp?" chỉ đánh giá *nội dung*
có đúng như mô tả, không đánh giá số dòng (số dòng sẽ trôi — xem ghi chú đầu tài liệu).

| Citation trong contract | Symbol thật | Khớp? | Ghi chú |
|---|---|---|---|
| `facebook.rs:42-44` | `graph_base` | **Khớp** | hardcode `https://graph.facebook.com/v19.0` |
| `facebook.rs:81` | `validate_facebook_token` | **Khớp** | `access_token` trong query của `/me` |
| `facebook.rs:136-140` | `list_facebook_pages` | **Khớp** | `access_token` trong query của `/me/accounts` |
| `facebook.rs:311-318` | `list_page_posts` | **Khớp** | `access_token` trong query của `/posts` |
| `facebook.rs:356-362` | `get_post_insights` | **Khớp** | `access_token` trong query của `/insights` |
| `facebook.rs:247` | `publish_content` (nhánh `/photos`) | **Khớp** | `.text("access_token", ...)` trong multipart |
| `facebook.rs:272` | `publish_content` (nhánh `/feed`) | **Khớp** | `access_token` trong form field |
| `facebook.rs:88-92`, `149`, `258`, `278`, `323`, `367` | 6 chỗ `.map_err(... "Không thể kết nối Graph API: {}", e)` | **Khớp** | mọi chỗ đều chèn nguyên `reqwest::Error` |
| `facebook.rs:54-69` | `map_graph_error` | **Khớp** | chỉ `190`/401 và `100`; không có `4/32/613/368/429` |
| `facebook.rs:197-199` | `publish_content` (validate) | **Khớp** | chỉ `page_id.trim().is_empty()` |
| `facebook.rs:207-215` | `publish_content` (resolve token) | **Khớp** | page token → user token |
| `security.rs:95-119` | `set_secret_hybrid` | **Khớp** | luôn `set_fallback` khi có AppHandle; comment "we still write fallback as backup" |
| `security.rs:17-23` | `fallback_path` | **Khớp** | `app_data_dir()/secure-credentials.json` |
| `facebook.rs:46-48` | `page_token_key` | **Khớp** | `fb_page_{id}` |
| `facebook.rs:162-163` | `list_facebook_pages` (cache) | **Khớp** | ghi `set_secret_hybrid` mỗi lần gọi (kể cả khi token không đổi) |
| `security.rs:199-202` | `get_facebook_token` | **Khớp** | trả `Option<String>` thô |
| `lib.rs:381` | `run().invoke_handler` | **Khớp** | `security::get_facebook_token` có đăng ký |
| `schedule.rs:84-88` | `SchedulerConfig::default` | **Khớp** | `interval_seconds: 30, enabled: true` |
| `schedule.rs:371-437` | `process_due_schedules` | **Khớp** | 1 vòng `for`, không sleep/jitter/cap; `Published` sau `await`; ghi index cuối hàm |
| `schedule.rs:425-435` | `process_due_schedules` (nhánh `Err`) | **Khớp** | `retry_count += 1` cho mọi lỗi, max 3 |
| `schedule.rs:358-364` | `retry_delay_minutes` | **Khớp** | 5 / 10 / 30 phút |
| `schedule.rs:10-12` | `ScheduleStatus` | **Khớp** | 4 biến thể, không có `Publishing` |
| `lib.rs:367-372` | `run().setup` | **Khớp** | spawn `start_scheduler` + `start_auto_scheduler` mỗi lần mở app |
| `App.jsx:412` | `SettingsStatus` | **Khớp** | `credential_status` + `get_active_provider`, 4s |
| `App.jsx:1405` | `SchedulePage` | **Khớp** | `list_schedule` + `list_content`, 4s (không gọi Graph) |
| `App.jsx:1548` | `DashboardSchedulePanel` | **Khớp** | `list_schedule` + `list_content`, 4s (không gọi Graph) |
| `App.jsx:1688` | `App` (effect dashboard) | **Khớp** | 4s, chạy suốt phiên, deps `[desktop, active, styleList]` |
| `App.jsx:1614-1641` | `App` | **Khớp** | `list_facebook_pages` + `list_page_posts` + 3× `get_post_insights` = 5 call |
| `App.jsx:1640`, `1667` | `App` | **Khớp** | `catch { return null }`, `catch { /* giữ '—' */ }` |
| `README.md:54` | bước 2 hướng dẫn token | **Khớp** | "nếu `code 190` → app đã bị xóa, tạo lại ở app khác" + "không vào JSON" (sai) |
| `README.md:56` | bước 4 | **Khớp** | "Dashboard sau 4s tự gọi `list_page_posts` + `get_post_insights`" |
| `.gitignore:1-10` | — | **Khớp** | không có `.env`; `git check-ignore -v .env` → exit 1 |
| `SECURITY.md` (fallback Linux-only) | mục "Lưu trữ secret" | **Khớp** | đúng là chỉ mô tả fallback trong dòng Linux |
| `package.json` scripts | `dev, tauri, tauri:dev, tauri:build, build, lint, e2e, e2e:ui` | **Khớp** | không có script test Rust riêng |
| `wiremock 0.6` là dev-dependency | `Cargo.toml` | **Khớp** | chỉ dùng trong `ai.rs`; không có test cho `/feed`, `/photos`, `/me/accounts` |
| `cfg!(test)` early-return | `publish_content` (2 chỗ), `list_page_posts`, `get_post_insights` | **Khớp** | ~219-221, 235-237, 306-308, 352-354 |
| Không có `app_secret`/`client_secret`/`appsecret_proof`/OAuth trong `src/` | — | **Khớp** | quét `src/`, `src-tauri/src/` → 0 kết quả |

**Citation lệch số dòng phát hiện thêm (không ảnh hưởng kết luận, chỉ củng cố việc dùng symbol):**

| Nơi trích | Trích sai | Thực tế |
|---|---|---|
| `README.md` bước 1–3 | `facebook.rs:128`, `facebook.rs:220`, `facebook.rs:327` | `list_facebook_pages`, `publish_content`, `get_post_insights` (lệch 9–17 dòng) |
| `README.md` bước 3 | `security.rs:14` cho chỗ cache `fb_page_{id}` | việc cache nằm ở `facebook.rs:list_facebook_pages` |
| `README.md` bước 4 | `App.jsx:984` cho fallback insights | dòng 984 là `list_facebook_pages` |
| `SECURITY.md` mục "Lưu trữ secret" | `facebook.rs:154` cho cache Page token | thực tế ở `list_facebook_pages` (dòng 163) |
