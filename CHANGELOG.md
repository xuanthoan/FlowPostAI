# Changelog

Tất cả thay đổi đáng chú ý của FlowPost AI được ghi lại ở đây. Định dạng theo [Keep a Changelog](https://keepachangelog.com/vi/1.0.0/) và dự án tuân theo [Semantic Versioning](https://semver.org/lang/vi/).

## [Unreleased] - 2026-10-19 — Bỏ giới hạn 4000 ký tự
### Changed
- `src/App.jsx` — bỏ `maxLength={4000}` ở ô Prompt, Nội dung style và Prompt mẫu; bộ đếm chỉ hiển thị số ký tự đã nhập (dạng `1234 ký tự`), không còn trần `/4000`.
- `src-tauri/src/ai.rs` — `DEFAULT_MAX_TOKENS` 2048 → 4096 (giữ `AI_MAX_TOKENS_CAP = 8192`); lần thử lại phát 8192 token.
### Removed
- `src-tauri/src/topics.rs` — bỏ `MAX_TEMPLATE_CHARS` và lỗi `Prompt mẫu tối đa 4000 ký tự`.
- `src-tauri/src/styles.rs` — bỏ trần 4000 ký tự của nội dung style và lỗi `Nội dung style tối đa 4000 ký tự`.
### Verified
- `npm run build` — `✓ built in 5.42s`, exit 0.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib` — `test result: ok. 187 passed; 0 failed`.

## [Unreleased] - 2026-10-19 — Tắt kiểm tra chính tả ở mọi ô nội dung
### Changed
- `index.html` — thêm `spellcheck="false"` trên thẻ `<html>` để mọi ô nhập kế thừa.
- `src/App.jsx` — thêm `spellCheck={false}` cho 19 ô nhập văn bản (`input` text + `textarea`); không đổi hành vi, placeholder, `maxLength`, `disabled` hay style; không đụng `type="password"` và `type="datetime-local"`.
### Verified
- `npm run build` — `✓ built in 13.45s`, exit 0.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib` — `test result: ok. 187 passed; 0 failed`.

## [Unreleased] - 2026-10-19 — Chủ đề + random placeholder (frontend)
### Added
- `src/App.jsx` — mục `Chủ đề` phía trên ô Prompt: select `Tự do`/chủ đề + `Sửa`/`Thêm chủ đề` + preview 1 dòng; modal chủ đề (tên ≤60, prompt mẫu ≤4000, editor danh sách "mỗi dòng 1 cụm từ"); dòng `Đã dùng:` hiển thị biến thể do Rust mở rộng.
- `e2e/topics.spec.ts` — 6 test cho `list_topics`/`create_topic`/`update_topic`/`delete_topic` qua mock.
### Changed
- `src/App.jsx` — ô Prompt 500 → 4000 ký tự; `generate_content` gửi thêm `topicLists`; `save_content` gửi thêm `topic` (tên chủ đề hoặc null); Kho nội dung tìm kiếm/hiển thị chủ đề và `update_content` truyền `topic` (tránh xoá chủ đề khi sửa bài).
### Verified
- `npm run lint` → `eslint .` chạy sạch, exit 0.
- `npm run build` → `✓ built in 10.08s`, exit 0.
- `E2E_MOCK=true npx playwright test --list` → `Total: 31 tests in 6 files`.

## [Unreleased] - 2026-10-19 — Chủ đề + random placeholder (backend)
### Added
- `src-tauri/src/topics.rs` — store `topics.json` (app_data_dir, atomic tmp+rename) với 4 command `list_topics`/`create_topic`/`update_topic`/`delete_topic` + validate (tên ≤60, template ≤4000, ≤10 danh sách, ≤50 cụm từ/danh sách, cụm từ ≤100 ký tự) + 24 unit test.
- `src-tauri/src/ai.rs` — `topicLists` (optional) trong `GeneratePayload`; engine `expand_prompt` cú pháp `[tên danh sách]` (SplitMix64, seed tiêm được khi test), cap 12000 ký tự; field `promptUsed` trong `GenerateResult` + 16 test.
- `src-tauri/src/content.rs` — `ContentItem.topic: Option<String>` (`serde(default)`, không cần migration) + 2 payload + `normalize_topic` + 4 test.
- `src-tauri/src/lib.rs` — đăng ký 4 command `topics::*`.
### Changed
- `generate_content` gọi LLM bằng prompt đã mở rộng; `GenerateResult.prompt` vẫn là template gốc (không đổi semantics).
### Verified
- `cargo check --manifest-path src-tauri/Cargo.toml` → `Finished` 0 lỗi.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib -- --nocapture --test-threads=1` → `test result: ok. 187 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out` (baseline 143 + 44).

## [Unreleased] - 2026-10-19 — Thu gọn khu vực cài đặt style trong trang Tạo nội dung AI
### Changed
- `src/App.jsx` — box cấu hình style luôn-hiện được thay bằng hàng gọn: dropdown Style + nút `Sửa` (icon `Pencil`) + nút `Thêm style` (icon `Plus`) + badge `Mặc định`/`Tùy chỉnh` + đúng 1 dòng preview nội dung (ellipsis); editor chuyển thành modal popup chứa `Tên style`/`Nội dung style` và các nút `Lưu thay đổi`/`Nhân bản`/`Khôi phục mặc định`/`Xóa`/`Đóng`; đóng modal không lưu (không gọi invoke) và re-sync draft. Không đổi payload gửi backend (`styleContent`, `style`/`customStyle: null`), không đổi Rust, E2E vẫn 25 test / 5 file.
### Fixed
- `src/App.jsx` — xoá style đang chọn không còn ghi đè `styleDraft` bằng style đã xoá (tránh preview/payload lệch); modal nhận focus khi mở để ESC đóng được ngay.

## [Unreleased] - 2026-10-19 — Xác minh & hiệu chỉnh tài liệu bảo mật API key
### Fixed
- `SECURITY.md` — sửa khẳng định sai ở mục "Lịch sử key": câu "Working tree cũng sạch (`.env` chưa tồn tại)" không đúng, vì `e2e/.env` tồn tại trên đĩa (untracked, bị `e2e/.gitignore` bỏ qua).
### Changed
- `SECURITY.md` — mục "Lịch sử key" phản ánh đúng trạng thái đã kiểm chứng: `git filter-repo` đã chạy, commit gốc chứa key (`8bd6850`) và các hash trước rewrite không còn tồn tại cục bộ, bản viết lại (`c0adfcb`) đã publish lên `origin/dev`; `.env` không được track.
### Verified
- `git log --all --name-only -S"sk-"` → chỉ khớp: placeholder `sk-REPLACE_WITH_YOUR_KEY` (`SECURITY.md`, `CHANGELOG.md`, `e2e/README.md`, `e2e/.env.example`, `e2e/helpers/provider.ts`) và các chuỗi rút gọn dạng `sk-...` trong test (`e2e/providers.spec.ts`); không có `sk-*` key thật trong ref reachable.
- `git rev-parse 8bd6850` → `fatal: ambiguous argument '8bd6850': unknown revision or path not in the working tree` (commit gốc không còn tồn tại cục bộ).
- `git log --oneline origin/dev..c0adfcb` → rỗng (bản rewrite đã lên `origin/dev`).
- `git ls-files --error-unmatch e2e/.env` → `error: pathspec 'e2e/.env' did not match any file(s) known to git`; `git log --all --name-only -- e2e/.env` → rỗng.
- `git status --porcelain --ignored -- e2e/` → `!! e2e/.env` (được ignore, không tracked).
### Notes
- Entry lịch sử `[370d1bb]` (2026-09-05) giữ nguyên; câu "cần filter-repo riêng" trong đó đã lỗi thời và được entry này supersede.
- CHƯA xác minh: trạng thái key trong `e2e/.env` trên đĩa (không đọc giá trị); việc GitHub còn phục vụ blob cũ theo SHA hay không (cần network/owner); số object unreachable (`git fsck` bị chặn trong sandbox).
- Còn một **đoạn tiền tố key đã bị cắt** (dạng `sk-xxxx…`, không đủ để sử dụng) trong blob lịch sử của `e2e/providers.spec.ts` ở commit `c0adfcb` (commit `462e2af` đã gỡ nó; `git log -S` vẫn liệt kê cả hai commit vì số lần xuất hiện thay đổi); đây không phải credential dùng được, nhưng KHÔNG thể xoá nếu không viết lại lịch sử lần nữa (cần quyết định riêng của chủ dự án).

## [Unreleased] - 2026-10-19 — Style có nội dung chỉnh sửa được + lưu style tùy chỉnh
### Added
- `src-tauri/src/styles.rs` — store `styles.json` trong app_data_dir: CRUD (thêm/sửa/nhân bản/xóa/khôi phục mặc định), seed 3 built-in cố định `viral`/`motivational`/`story`, tự chữa lành (self-heal: dedupe id, seed lại built-in bị xóa, đặt lại entry mặc định bị rỗng), 21 unit test (gồm test chống mojibake).
- `src-tauri/src/lib.rs` — đăng ký 6 command `styles::list_styles`, `styles::create_style`, `styles::update_style`, `styles::duplicate_style`, `styles::delete_style`, `styles::reset_style`.
- `e2e/styles.spec.ts` — 6 test E2E (mock stateful) cho `list_styles`/`create_style`/`update_style`/`duplicate_style`/`delete_style`/`reset_style`.
### Changed
- `src-tauri/src/ai.rs` — `GeneratePayload.styleContent`; `build_system_message` nhận thêm khối hướng dẫn style (áp dụng giọng văn/cấu trúc, quy tắc định dạng luôn thắng khi xung đột), giữ nguyên hành vi byte-for-byte khi không có nội dung; thêm 2 test.
- `src/App.jsx` — chọn style hiển thị đầy đủ nội dung cấu hình để xem/sửa; thêm/nhân bản/xóa/khôi phục mặc định; bỏ tùy chọn `Custom` + ô `customStyle` ở trang AI; `generate_content` gửi `styleContent`; `save_content` lưu TÊN style và `customStyle: null`.
- `e2e/ai.spec.ts` — mock `generate_content` echo thêm `styleContent`.
### Fixed
- `src-tauri/src/styles.rs` — sửa lỗi toàn bộ chuỗi tiếng Việt bị mojibake (double-encoded) trong bản đầu tiên; thêm test `test_vietnamese_literals_are_not_mojibaked` (chỉ dùng escape ASCII nên không thể bị hỏng theo) để chặn tái phát.
### Verified
- `cargo check --manifest-path src-tauri/Cargo.toml` → `Finished` 0 lỗi (7 warning dead-code có sẵn, không phát sinh mới).
- `cargo test --manifest-path src-tauri/Cargo.toml --lib -- --nocapture --test-threads=1` → `test result: ok. 143 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.44s` (120 cũ + 21 styles + 2 ai).
- `npm run lint` → exit 0 (không có output).
- `npm run build` → `✓ 2395 modules transformed` / `✓ built in 7.00s`.
- `npx playwright test --list` → `Total: 25 tests in 5 files` (19 cũ + 6 styles).
### Notes
- Nội dung style do người dùng sửa được lưu vào `styles.json` trong app_data_dir (không dùng localStorage).
- `content.rs` và dữ liệu nội dung đã lưu giữ nguyên (không migration); `ContentItem.style`/`custom_style` vẫn lưu TÊN style.
- Giao diện thật cần `npm run tauri:dev` để kiểm chứng.

## [Unreleased] - 2026-10-19 — Bỏ tính năng chọn độ dài bài viết (Ngắn/Vừa/Dài)
### Changed
- `src-tauri/src/ai.rs` — bỏ `length` khỏi `GeneratePayload`, xoá `max_tokens_for_length`, cố định `DEFAULT_MAX_TOKENS = 2048` (giữ nguyên hành vi cũ vì trước đây nhánh mặc định cũng là 2048); cập nhật `EMPTY_BODY_ERROR` (bỏ câu "chọn độ dài khác").
- `src/App.jsx` — bỏ dropdown "Độ dài" ở trang AI Content; grid 3 cột → 2 cột; payload `generate_content` không còn gửi `length`.
- `e2e/ai.spec.ts` — bỏ `length` khỏi payload test và xoá test `generate_content respects length Ngắn/Dài` (đã lỗi thời).
### Verified
- `cargo check --manifest-path src-tauri/Cargo.toml` → `Finished` 0 lỗi (7 warning dead-code có sẵn, không phát sinh mới).
- `cargo test --manifest-path src-tauri/Cargo.toml --lib -- --nocapture --test-threads=1` → `test result: ok. 120 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.56s` (119 cũ − 1 xoá + 2 mới).
- `npm run lint` → exit 0 (không có output).
- `npm run build` → `✓ 2395 modules transformed` / `✓ built in 11.39s`.
- `npx playwright test --list` → `Total: 19 tests in 4 files` (giảm 1 test đã xoá).
### Notes
- Độ dài bài viết giờ do người dùng mô tả trong prompt; `customStyle` và nhánh `Custom` vẫn giữ nguyên (sẽ thay đổi ở phần Style sau).

## [Unreleased] - 2026-10-18 — Fix AI trả về tiêu đề cụt + thân bài rỗng (Gemini thinking budget)
### Fixed
- `src-tauri/src/ai.rs` — Gemini 2.5/3.x là thinking model; trên endpoint OpenAI-compat, `max_tokens` bị tính chung cho cả phần suy luận lẫn output hiển thị nên 250/500/800 token bị "đốt" hết vào suy luận → model cắt còn đúng một dòng tiêu đề dở (`finish_reason:"length"`) và thân bài rỗng, nhưng app vẫn báo "Đã tạo nội dung". Nay app hạ `reasoning_effort` cho host Gemini, phát hiện `finish_reason == "length"`, thử lại 1 lần với ngân sách token gấp đôi, và báo lỗi tiếng Việt rõ ràng thay vì "thành công giả" khi thân bài rỗng
### Changed
- `src-tauri/src/ai.rs` — `max_tokens` theo độ dài: 250/500/800 → 1024/2048/4096 (Ngắn/Vừa/Dài); thêm trần `AI_MAX_TOKENS_CAP = 8192`
- `src-tauri/src/ai.rs` — gửi `reasoning_effort: "low"` CHỈ tới host Google Gemini (`generativelanguage.googleapis.com`); provider khác không gửi field này
- `src-tauri/src/ai.rs` — `parse_ai_response` nhận thêm separator 2 dòng trống (`\n\n\n`) đúng như system prompt yêu cầu (trước đây chỉ tách theo `\n---\n` hoặc dòng đầu)
### Verified
- `cargo test --lib` → `test result: ok. 119 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s` (103 cũ + 16 mới, gồm 1 test HTTP thật bằng wiremock cho retry); `cargo check` 0 lỗi (7 warning dead-code có sẵn); `cargo build` OK; `npm run build` → `✓ 2395 modules transformed` / `✓ built in 12.47s`; `npm run lint` exit 0 (do implementer chạy); Playwright `--list` OK (20 test)
### Notes
- Hành vi Gemini không thể xác minh offline nếu không có API key thật — người dùng cần chạy `npm run tauri:dev` để kiểm chứng end-to-end.
- Khối xử lý mojibake giữ nguyên byte-for-byte; marker `MOJIBAKE_REPAIR_VERSION = "v3"` vẫn hoạt động.

## [Unreleased] - 2026-10-18 — Onboarding provider AI theo chuẩn OpenAI Compatibility (preset + fix fetch/save models)
### Added
- `src/providerPresets.js` — catalog 6 preset: ChatGPT (OpenAI), Google Gemini, Anthropic Claude, DeepSeek, Muse (Meta Model API), Custom/OpenAI-compatible + helper `isDefaultLocalBase`
- `src/App.jsx` `ProvidersPanel` — dropdown "Mẫu provider nhanh" điền sẵn base URL + model gợi ý (không ghi đè id/display name đang có); badge `● active` phản ánh provider đang dùng; nút `Đặt active` trên mỗi dòng chưa active; nút lưu đổi thành `Tạo provider`/`Cập nhật provider` (bỏ toggle Edit/Create mập mờ)
- `src-tauri/src/providers.rs` — command `set_active_provider` (chọn provider đang dùng) + đăng ký trong `lib.rs`
### Fixed
- `src-tauri/src/providers.rs` — `fetch_provider_models` trước đây GET thẳng base URL (không thêm `/models`) nên Gemini/OpenAI/DeepSeek trả 404; nay dựng URL qua `models_url()` (chuẩn hoá base, bỏ các suffix endpoint đã biết) và FALLBACK về base URL gốc khi `/models` không dùng được (giữ tương thích gateway local hiện tại)
- `src-tauri/src/providers.rs` — `parse_models_response` hỗ trợ thêm format Google `{"models":[{"name":"models/..."}]}` (tự strip tiền tố `models/`), format OpenAI `{"data":[{"id":...}]}` và mảng top-level
- `src-tauri/src/providers.rs` — `update_provider` trước đây BỎ QUA khi danh sách models rỗng nên không thể sửa/xoá model; nay ghi kể cả mảng rỗng (qua `apply_update_fields`)
- `src-tauri/src/providers.rs` — `create_provider` trước đây luôn ép `xoay-vong-worker-web-128k` cho mọi provider; nay chỉ ép khi base URL là gateway local mặc định
- `src/App.jsx` — "Fetch available models" trước đây TRỘN model mới vào danh sách cũ (tích tụ model sai); nay THAY THẾ (có xác nhận khi danh sách hiện tại khác rỗng)
- `src/App.jsx` — `load()` trước đây không set `activeId` nên badge `● active` không bao giờ hiện
### Changed
- `src-tauri/src/ai.rs` — dùng chung `providers::chat_completions_url()` thay vì tự cắt suffix inline (một nguồn chuẩn hoá URL cho cả generate và fetch models)
- `e2e/providers.spec.ts` — mock có state `activeId` + case `set_active_provider` + 1 test mới; `e2e/schedule.spec.ts` thêm case phòng vệ
### Verified
- `cargo test --lib` 103 passed / 0 failed (91 cũ + 12 mới); `cargo check`/`cargo build` OK (7 warning dead-code có sẵn, không phát sinh cảnh báo mới); `npm run build` OK (2395 modules); `npm run lint` exit 0 (do implementer chạy; QA bị chặn bởi sandbox allowlist nên không chạy lại độc lập được); Playwright `--list` parse OK (8 test providers). Chạy e2e runtime cần cửa sổ Tauri (`npm run tauri:dev`) → chưa xác minh.
### Notes
- Provider luôn dùng `protocol = openai-completions` (Gemini/Claude/DeepSeek/OpenAI đều hỗ trợ chuẩn này); chưa expose UI chọn protocol.
- "muse" = preset Muse (Meta Model API): base URL https://api.meta.ai/v1, chuẩn OpenAI-compatible (chat/completions), Bearer `MODEL_API_KEY`; model gợi ý muse-spark-1.3 / muse-spark-1.1 (tham khảo).
- Model gợi ý trong preset chỉ mang tính tham khảo; nút "Fetch available models" là nguồn chân lý.
- Base URL nhập ở dạng nào cũng được (`https://api.deepseek.com` hay `.../v1`, có/không `/chat/completions`) vì cả generate và fetch models dùng chung một hàm chuẩn hoá.

## [Unreleased] - 2026-10-17 — Fix triệt để mojibake output AI (encoding)
### Fixed
- `src-tauri/src/ai.rs` — thay `String::from_utf8_lossy` (tạo U+FFFD, phá byte) bằng `decode_response_bytes` không mất dữ liệu: UTF-8 nghiêm ngặt, fallback giải mã CP1252 (byte→char)
- `src-tauri/src/ai.rs` — viết lại `repair_mojibake` theo segment-wise: mỗi đoạn ký tự map-được CP1252 được giải mã độc lập; ký tự không map được (U+FFFD, emoji, chữ Việt đúng) đi xuyên qua và KHÔNG chặn việc phục hồi các đoạn lân cận (trước đây guard all-or-nothing khiến 1 ký tự lỗi khoá toàn bộ tiêu đề + thân bài)
- `src-tauri/src/ai.rs` — `repair_mojibake` lặp tối đa 2 lượt (`MOJIBAKE_MAX_PASSES`) để xử lý double-encoding; idempotent, không đổi văn bản Việt/ASCII đúng
- `src-tauri/src/content.rs` — `read_content_index` phục hồi mojibake khi ĐỌC (in-memory, KHÔNG ghi lại file) để bài viết cũ trong Kho nội dung hiển thị đúng
### Added
- `src-tauri/src/ai.rs` — marker `MOJIBAKE_REPAIR_VERSION = "v2"` + log khởi động trong `lib.rs run()` và log mỗi lần `generate_content` (chứng minh build mới đang chạy)
- `src-tauri/src/ai.rs` — chẩn đoán `looks_like_mojibake` + `log_mojibake_preview` (giới hạn 64 byte hex, chỉ in khi còn mojibake sau khi sửa; không in prompt/header/key)
- `src-tauri/src/ai.rs` — 9 test mới (round-trip, segment quanh U+FFFD, idempotent, double-layer, decode UTF-8/CP1252, pipeline decode→repair)
### Verified
- `cargo test --lib` 85 passed / 0 failed (76 cũ + 9 mới); `cargo check` 0 errors; `npm run build` OK. `npm run lint` bị chặn bởi quyền sandbox trong phiên QA (diff thuần Rust nên không ảnh hưởng ESLint). Kiểm thử GUI end-to-end với AI thật: người dùng cần rebuild và xác nhận.
- **Round 3 (v3) — partial-run salvage**: `flush_run` giờ cứu được tiền tố UTF-8 hợp lệ dài nhất của một "run" hỏng, thay vì bỏ luôn cả run. Nguyên nhân còn sót: gateway (`127.0.0.1:20128`) giải mã UTF-8 sai sang CP1252 và biến 5 byte CP1252 không xác định (0x81/0x8D/0x8F/0x90/0x9D) trong chuỗi UTF-8 tiếng Việt thành `U+FFFD`, khiến run ngay cạnh bị cụt (dangling lead byte) và trước đây bị bỏ toàn bộ. Guard `improved` (chỉ nhận khi tiền tố chứa ký tự > U+00FF) đảm bảo văn bản Latin-1 hợp lệ (`café – test`, `Résumé – déjà vu`, `naïve façade`) không bị sửa. Marker `MOJIBAKE_REPAIR_VERSION = "v3"`. Bằng chứng: `cargo test --lib` 91 passed / 0 failed; 6 test mới dựa trên 64 byte hex thật lấy từ log app; `cargo check` 0 lỗi; chỉ `src-tauri/src/ai.rs` thay đổi.
- **Giới hạn không thể tránh**: các ký tự mà gateway đã mất byte (hiển thị `�`) KHÔNG thể phục hồi (ví dụ `Đ` trong `ĐỒ`); phần văn bản còn lại được phục hồi đầy đủ. Khuyến nghị sửa tận gốc: cấu hình gateway/proxy trả response UTF-8 đúng.

## [Unreleased] - 2026-10-17 — Fix sửa bài viết ở Kho nội dung
### Fixed
- `src/App.jsx` — lỗi `update_content missing required key payload`: bọc tham số trong `{ payload: { contentId, title, body, prompt, style, customStyle, mediaIds } }` (khớp `save_content`)
- `src/App.jsx` — kéo dãn textarea rồi thả chuột bị đóng preview: overlay chuyển từ `onClick` sang `onMouseDown` + guard `e.target === e.currentTarget` (chỉ đóng khi bấm trực tiếp ra nền)
- `src/App.jsx` — lỗi `unknown variant Approved`: chuẩn hóa status về chữ thường `draft/approved/archived`; thêm `CONTENT_STATUS_LABELS`/`contentStatusLabel`; sửa filter ('all'), bộ đếm, badge, nút Duyệt/Lên lịch, dashboard `pending`
- `e2e/schedule.spec.ts` — status literals chuyển về chữ thường cho khớp serde backend
### Added
- `src-tauri/src/content.rs` — 4 test: serialization lowercase tuyệt đối, deserialization đúng giá trị frontend (từ chối PascalCase), persisted JSON lowercase, `UpdateContentPayload` camelCase; thay test cũ quá dễ dãi
### Verified
- `cargo test --lib` 68 passed / 0 failed; `cargo check` 0 errors (7 warning dead-code có sẵn); `npm run build` OK (2394 modules). `npm run lint` bị chặn bởi quyền sandbox (chưa xác minh độc lập). Kiểm thử GUI thủ công (AC11) chưa chạy.

## [Unreleased] - 2026-10-17 — AI content Facebook-ready formatting
### Added
- `src-tauri/src/content.rs` — `TITLE_BODY_SEPARATOR = "\n\n\n"` + `compose_facebook_post(title, body)` (ghép tiêu đề + 2 dòng trống + thân bài; xử lý rỗng) + 2 tests
- `src-tauri/src/ai.rs` — `strip_markdown` (loại bỏ `#`/`**`/`*`/`_`/`` ` ``/`>`/bullet, không dùng regex); `build_system_message` thêm 5 quy tắc định dạng (plain text, KHÔNG markdown, tiêu đề IN HOA, cách 2 dòng); `parse_ai_response` chuẩn hóa title=UPPERCASE + strip markdown; thêm tests
### Changed
- `src-tauri/src/schedule.rs` — publish dùng `compose_facebook_post(&found.title, &found.body)` tại `read_content_with_media` + `read_content_body` (trước đây tiêu đề bị bỏ, chỉ đăng body)
- `src/App.jsx` — preview body thêm `whiteSpace:'pre-line'` để giữ xuống dòng/2 dòng trống
### Fixed
- `###`/`**` hiển thị nguyên văn trên Facebook → nay AI bắt buộc plain text, có lớp phòng thủ strip markdown ở Rust
- Tiêu đề không còn dính liền thân bài (trước đây newline bị gộp và tiêu đề không được đăng)
- Gộp các fix pipeline AI có sẵn trong working tree: parse SSE streaming (`data:`/`[DONE]`), endpoint `/chat/completions` (tránh double suffix), `localhost`→`127.0.0.1` (tránh IPv6), `AI_TIMEOUT_SECS` 120s, log lỗi; `providers.rs` base URL `127.0.0.1` + `.http1_only()`
### Verified
- `cargo test --lib` 65 passed / 0 failed; `cargo check` 0 errors; `npm run build` OK (2394 modules). `npm run lint` bị chặn bởi quyền công cụ (chưa xác minh độc lập trong phiên QA).

## [1be13e2] - 2026-09-05 — B1 CI + lint fixes
### Added
- `.github/workflows/ci.yml` — single-job CI (ubuntu-latest, Node 20, npm ci, lint, build, Rust stable + cargo check/test, Playwright E2E_MOCK 19 tests, Vite dev server, upload artifact on failure)
### Fixed
- `src/App.jsx` — 7 lỗi eslint (di chuyển `selectedPageId` trước `refresh`, `catch (_e) { void _e }`, disable `set-state-in-effect`), `npm run lint` pass, `npm run build` success

## [d8987ba] - 2026-09-05 — A4 reschedule/cancel + A5 E2E publish/schedule mock
### Added
- `src/App.jsx` SchedulePage — `platformFilter` (Tất cả/3 platforms), badge `Cancelled` xám, Queue 3 nút Dời/Hủy/Xóa, Calendar dayDetail + rescheduleFor modal, `handleReschedule` → `reschedule`, `cancelItem` → `update_schedule_status Cancelled`
- `e2e/publish.spec.ts` (4 tests) — mock `validate_facebook_token`, `list_facebook_pages`, `publish_content` (check `.exe`)
- `e2e/schedule.spec.ts` (5 tests) — in-memory `mockContents`/`mockSchedules`, mock `create_schedule` (Approved + 5m + pageId), `reschedule`, `Cancelled`
### Changed
- E2E suite từ 8 lên 19 tests (ai 3, providers 6, publish 4, schedule 5) qua `E2E_MOCK=true`

## [a569771] - 2026-09-05 — A3 scheduler hardening
### Added
- `src-tauri/src/schedule.rs` — `resolve_first_image_path` (đọc `media-index.json`, check image/mime/10MB, fallback `None`), `read_content_with_media`, helper `first_image` cho `process_due_schedules`
- `src-tauri/src/schedule.rs` — `AtomicBool SCHEDULER_RUNNING` dedupe `start_scheduler`, hot-reload `interval_seconds` mỗi tick, `set_scheduler_config` validate `1..=3600`
### Fixed
- `process_due_schedules` trước luôn `publish_content(..., None)` bỏ qua ảnh — nay gửi ảnh đầu nếu có

## [2ca7c30] - 2026-09-05 — A2 wiring pageId
### Added
- `src-tauri/src/schedule.rs` — `CreateSchedulePayload` thêm `page_id: Option<String>`, `create_schedule` resolve `pageId = pageId.trim().or(pages.first())`, giữ backward compat JSON cũ
- `src/App.jsx` `ScheduleModal` — dropdown `PageInfo[]` từ `list_facebook_pages`, default `get_scheduler_config.selectedPageId`, gửi `pageId` + `pages:[pageId]`, validate `pageId` trống
- `src/App.jsx` `SettingsPage` — selector `Trang mặc định cho Lịch` gọi `select_scheduler_page`, persist `schedule-config.json`

## [3b6a43d] - 2026-09-05 — A1 PublishModal picker 1 ảnh đầu
### Added
- `src/App.jsx` `PublishModal` — picker 1 ảnh (`list_media` + `localAssetUrl`, `selectedMediaId`, `showPicker`), `handlePublish` lấy `found.path` truyền `image_path` (giữ `null` nếu không chọn), kích hoạt nhánh `facebook.rs:191` `photos` trước đó chết

## [a7b622f] - 2026-09-05 — Multi-provider + API thật
### Added
- `src-tauri/src/providers.rs` — bỏ MVP limit, `DEFAULT_PROTOCOL`, 3 protocols `openai-completions/responses`, `anthropic-messages`, cho phép multi-provider `create`, 3 tests mới
- `src-tauri/src/ai.rs` — routing 3 protocols (`endpoint_for_protocol`: `/chat/completions`, `/responses` với `instructions`, `/messages` với `system` + `x-api-key`), 5 tests mới (`endpoint_for_protocol`, `responses/anthropic serialization`, `parse responses/anthropic`)
- `src/App.jsx` — enable 3 protocols dropdown, xóa Mock demo notice, subtitle cập nhật, `setMsg('Đã tạo nội dung')`
- `e2e/providers.spec.ts` — mock hỗ trợ 3 protocols, thêm 2 tests multi-provider/anthropic
- `.gitignore` — thêm `test-results/`, `playwright-report/`
### Fixed
- `cargo check` 0 warning, `cargo test` 51 passed (tăng từ 46)

## [31874ad] - 2026-09-05 — E2E_MOCK mode
### Added
- `e2e/ai.spec.ts`, `e2e/providers.spec.ts` — `window.__TAURI_MOCK_IMPL__` qua `addInitScript`, cho phép `E2E_MOCK=true npx playwright test` không cần `tauri-driver`, `helpers/tauri.ts` + `provider.ts`

## [370d1bb] - 2026-09-05 — Security key hygiene
### Fixed
- Xóa API key thật khỏi git working tree (vẫn tồn tại trong history `8bd6850`, cần filter-repo riêng)

## [0.3.0] - 2026-09-06
### Changed
- Promote `0.3.0-alpha` → `0.3.0` stable: đồng bộ `package.json`/`Cargo.toml`/`tauri.conf.json` sang `0.3.0`, không đổi code, CI đã xanh `73760b8`+`a561ff4`

## [0.3.0-alpha] - 2026-09-06
### Added
- `LICENSE` MIT 2026 FlowPost AI, `SECURITY.md` bảng hỗ trợ version + lưu secret qua keyring
- `src/App.jsx` Dashboard — C1.1 local aggregation (bỏ mock `5.660`/`12.4K`), C1.2+1.3 `list_page_posts`/`get_post_insights` 6 metric, C2 tooltip 2 cột + `StatCard` tổng tương tác từ insights
- `src/App.jsx` Dashboard — `Tỷ lệ tương tác` từ insights (`totalEngaged/totalUnique*100`, `5a267a9`), fallback `—` khi chưa có token
- `src/App.jsx` fix(providers) — cô lập lỗi tab `code 190`, list card `Danh sách Providers`, `activeId`/`+ Thêm Provider`
- `README.md` — hướng dẫn lấy Facebook Token 5 scopes (`a561ff4`) sau blockquote Tauri
### Changed
- Version `0.3.0-alpha` đồng bộ `package.json`/`Cargo.toml`/`tauri.conf.json`, badge CI `branch=dev` (sẽ chuyển `main` ở R3)
- CI đã xanh `73760b8` (cargo test 51, E2E_MOCK 19) sau fix `libdbus` + `facebook mock` + `E2E dummy key`
