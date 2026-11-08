# Changelog

All notable changes to Hermio. Versions before 0.5.0 were released under the name **Nelo**
(entries kept in their original Vietnamese).

## 0.5.1

New-mail notifications that went missing:

- **Toasts are now shown by the sync itself**, not by the window. Before, they depended on the
  web page hearing the "new-mail" event, so they were lost during start-up (the first sync runs
  before the page is ready) and could be lost while the window sat hidden in the tray.
- **Hermio registers its app ID with Windows** at start-up. Windows drops toasts from an
  unregistered app, which hit the portable copy (and installs without a Start-menu shortcut).
  Toasts now also show the Hermio name and icon.
- A burst of mail (more than 3 at once) gives one summary toast instead of a flood, which
  Windows tends to swallow.
- New option *Also notify for mail sorted into groups*. Mail that rules or the AI put into a group
  hidden from the inbox used to arrive silently, with no way to change that.
- *Send a test notification* button (Settings → Mail) to check Windows' own notification settings.
- The sync log now says how many new messages were announced, and why the others weren't
  (sorted into hidden groups, marked read by a rule).
- POP3: mail older than two days that is still being downloaded from a large mailbox no
  longer triggers notifications.

## 0.5.0 — Hermio

- **New name and identity: Hermio – Automate your inbox flow.** New logo, app icon and a blue
  colour scheme taken from the logo across the whole app (title bar, sidebar, lists, settings,
  dialogs), in both light and dark themes. Default accent is Hermio blue; AI features use a
  matching blue-violet.
- Settings redesigned: gradient title, tinted navigation with an animated indicator, a brand
  strip on the content panel, new theme thumbnails, Hermio-blue accent presets and a new About
  header.
- Menu and sidebar text sizes tightened (folders, settings navigation, navigation rail, menus).
- Icons: every icon is optically centred in its box, and icons placed inline with text now sit on
  the text's centre line instead of breaking the line.
- Smoother motion: spring-like easing for menus, pages, tabs, switches and selection indicators.
- **Upgrading from Nelo:** the installed app moves Nelo's data (`%APPDATA%\app.nelo.mail`) to
  Hermio's folder on first start and keeps your passwords and tokens (they are moved from
  "Nelo Mail" to "Hermio Mail" in Windows Credential Manager on first use). Portable copies:
  *Settings → Backup & transfer → Copy data from an older copy*. Configuration and backup files
  made by Nelo can still be imported.
- Open source under MIT OR Apache-2.0; Windows builds run on GitHub Actions.

## 0.4.11

- **Gom nhóm theo nội dung**: AI đọc nội dung từng email và gán chủ đề (Marketing, Promotions, Newsletters, Security, Receipts & finance, Thesis & research, Jobs, Travel, Spam…). Một nhóm có thể gồm nhiều người gửi và nhiều kiểu nội dung khác nhau.
- **Quy tắc tự sinh và được kiểm chứng**: Nelo dựng quy tắc từ các người gửi / tên miền chủ yếu gửi loại thư đó và các từ khoá tiêu đề đặc trưng, rồi loại bỏ điều kiện bắt nhầm nhiều thư khác. Thẻ gợi ý cho biết quy tắc bắt được bao nhiêu email trong nhóm và bao nhiêu email ngoài nhóm.
- Hoạt động với cả mô hình NPU nhỏ (xử lý theo lô 8 email) lẫn mô hình đám mây; nút Dừng có tác dụng ngay.

## 0.4.10
- **Sửa lỗi NPU “connection aborted (os error 10053)”**: Nelo không còn gửi lệnh huỷ tới FastFlowLM khi chỉ dừng việc phân loại nền (lệnh đó có thể huỷ nhầm câu hỏi của bạn đang chờ ngay sau); khi kết nối tới runtime bị đứt, Nelo kiểm tra, khởi động lại runtime nếu cần và gửi lại một lần; kết nối tới máy chủ cục bộ không còn được dùng lại. Thông báo của FastFlowLM được ghi vào `npu-server.log` và kèm vào lỗi nếu runtime dừng bất thường.
- **Chuyển toàn bộ dữ liệu sang bản mới** (Cài đặt → Backup & transfer → Move all data): *Chép dữ liệu từ Nelo cũ* (chọn thư mục Nelo portable cũ hoặc thư mục data), *Tạo/Khôi phục bản sao lưu đầy đủ* (.zip: cơ sở dữ liệu, thư đã tải, thư POP3, mật khẩu mã hoá DPAPI). Dữ liệu được chuẩn bị trước, Nelo khởi động lại để thay vào; dữ liệu cũ của bản hiện tại được giữ trong `before-restore-…`; đường dẫn thư trong cơ sở dữ liệu được sửa theo thư mục mới.

## 0.4.9
- Cập nhật Google OAuth client mặc định (client mới của dự án nelo-mailbox).

## 0.4.8
- **Bộ lọc thời gian của trợ lý AI**: chỉ lấy thư *nhận* trong khoảng ngày đã chọn (bỏ Sent/Drafts khi tóm tắt, báo cáo, gom nhóm); “All inboxes” chỉ đọc các Inbox thay vì mọi thư mục. Với model NPU/local, thư hàng loạt (no-reply, newsletter, thông báo, hoặc ≥ 6 thư cùng người gửi) được tóm tắt qua tiêu đề, model chỉ đọc kỹ thư cá nhân → nhanh hơn nhiều. Câu trả lời ghi rõ khoảng ngày của thư đã đọc.
- **Nút Stop dừng ngay với mọi model**, kể cả khi model local/NPU đang nạp hoặc đang đọc prompt dài (trước token đầu tiên); các bước đọc theo lô cũng dừng. FastFlowLM được gửi lệnh huỷ — nếu không xác nhận, server được tắt để NPU ngừng hẳn (model nạp lại ở câu hỏi sau). Claude Code CLI cũng bị dừng.
- **NPU “vẫn chạy sau khi trả lời”**: nguyên nhân là việc phân loại hàng chờ thư mới chạy ngay sau câu trả lời. Giờ nó chỉ chạy khi bạn để NPU yên 15 giây, tự nhường khi bạn hỏi gì đó, và hiển thị chip “NPU đang phân loại N email” (có nút dừng) trong bảng AI.
- **About** hiển thị đúng phiên bản (lấy tự động từ mã nguồn) và ngày build.
- **Giao diện**: ảnh xem trước giao diện mới (cửa sổ Nelo thu nhỏ; System = nửa sáng/nửa tối), ô màu tuỳ chỉnh là vòng bánh xe màu bao quanh màu đã chọn; màu cuối thay bằng xám thép (màu cũ gần như vô hình trên nền tối).
- **Sao lưu & chuyển cài đặt** (Cài đặt → Backup & transfer): xuất toàn bộ cấu hình ra file JSON (hộp thư, không gian, nhóm, quy tắc, chữ ký, lịch đăng ký, danh bạ, AI, mọi tuỳ chọn; tuỳ chọn kèm mật khẩu/API key được mã hoá DPAPI theo tài khoản Windows, và các cuộc trò chuyện AI) rồi nhập vào bản mới — gộp thông minh, tự đổi lại các id tham chiếu.

## 0.4.7
- **Nhật ký đồng bộ**: bấm nút đồng bộ cạnh “Assistant” để đồng bộ và mở bảng **Trạng thái đồng bộ** — từng hộp thư (đang đồng bộ / lần cuối / lỗi đầy đủ) và nhật ký trực tiếp (kết nối, đăng nhập, số thư trên máy chủ, thư mới, thư bị bỏ qua, nội dung đã tải, luật đã áp…), lọc theo hộp thư hoặc chỉ lỗi, sao chép, mở file `sync.log` trong thư mục dữ liệu. Chấm đỏ trên nút khi có hộp thư lỗi. F5 vẫn đồng bộ mà không mở bảng.
- **Sửa lỗi Inbox không đồng bộ dù đăng nhập thành công**: trước đây lỗi khi đồng bộ Inbox chỉ được ghi ra console (không hiển thị) và một thư máy chủ trả về hỏng làm hỏng cả lô → Inbox trống mãi. Giờ tiêu đề thư được tải theo lô 100, lô lỗi thì thử từng thư (kết nối lại), thư không đọc được bị bỏ qua và ghi vào nhật ký; lỗi Inbox được báo trên hộp thư; đăng nhập/SELECT có thời gian chờ thay vì treo.
- **Thư mục model có sẵn**: chọn thư mục đang chứa model FastFlowLM (hoặc thư mục cha của nó, ví dụ `.flm`) thì các model trong đó được nhận ngay — chọn làm mặc định, dùng trong chat, xoá (kể cả model không do Nelo tải) và **tải lại** (xoá file rồi tải mới). Intel: nhận cả model OpenVINO tự tải về dạng `<thư mục>/<tên model>`. Nelo báo số model nhận được và các file định dạng khác mà NPU không dùng được (GGUF, safetensors…).

## 0.4.6
- **Tóm tắt / Báo cáo đọc toàn bộ email trong phạm vi đã chọn** (hộp thư/nhóm/thư mục + khoảng ngày), không còn giới hạn 15 (NPU) hay 40 (cloud) email. Nếu vừa một lượt, mỗi email được cấp phần nội dung tối đa theo dung lượng ngữ cảnh của model; nếu không vừa, Nelo đọc theo lô (mỗi email một dòng ghi chú có số trích dẫn [E<n>]), gộp ghi chú khi vẫn quá dài, rồi mới viết bản tóm tắt cuối. Có thanh tiến trình “Đang đọc n/N email…”, nút Stop dừng được giữa chừng; tối đa 3.000 email mỗi lần.

## 0.4.5
- **Tìm theo người**: chế độ tìm mới **People** (Subject · Content · People) tìm trong tên/địa chỉ người gửi, danh sách **To** và **Cc** (không phân biệt dấu, khớp tiền tố). Ở mọi chế độ có thể dùng thêm bộ lọc `from:`, `to:`, `cc:` — ví dụ `to:minh@monash.edu`, `from:"Lan Pham" báo cáo`.
- **Kết quả tìm kiếm có đoạn trích**: mỗi kết quả hiện 2 dòng văn bản quanh chỗ khớp (từ nội dung thư, hoặc dòng From/To/Cc có nhãn), từ khoá được tô màu đậm như trong ảnh mẫu; áp dụng cho cả tiêu đề, người gửi, To/Cc trong khung đọc thư.
- **Chuyển động mượt hơn ở khắp ứng dụng**: thư mới/xoá trong danh sách mờ dần vào/ra và các dòng khác trượt vào chỗ; đổi thư mục/tìm kiếm có hiệu ứng xuất hiện lần lượt; con trỏ chọn trượt giữa các lựa chọn (Subject/Content/People, Ngày/Tuần/Tháng…); số chưa đọc đếm lên/xuống; bảng AI trượt ra và khung thư co lại theo (không giật); nội dung thư hiện dần sau khi tải; Cc/Bcc, tệp đính kèm, người nhận, cài đặt nâng cao, thông báo, quy tắc, nhóm… đều có hiệu ứng mở/đóng; lịch trượt khi chuyển tuần/tháng; khối `<details>` (Thinking) mở/đóng mượt.
- **Streaming AI mượt**: chữ hiện đều theo khung hình (tự đuổi kịp khi model trả về từng cục lớn), mỗi đoạn mới mờ dần vào, con trỏ nhấp nháy đứng ngay sau chữ cuối; áp dụng cho câu trả lời, phần Thinking và bản nháp trả lời.
- **Trích dẫn email chính xác hơn, kể cả với model NPU nhỏ**: mỗi email trong ngữ cảnh được đánh số ngắn [E1], [E2]… thay cho id trong cơ sở dữ liệu; ví dụ mẫu trong prompt của model nhỏ chỉ còn là khuôn trống (trước đây model 1B chép nguyên ví dụ “Invoice from Acme #41…” vào câu trả lời); trích dẫn sai/không tồn tại bị loại bỏ, `[#id]` hoặc `[e3]` được chuẩn hoá.
- **Mở email theo số trích dẫn**: số trích dẫn hiện thành huy hiệu tròn; dưới mỗi câu trả lời có danh sách các email được trích (số · người gửi · tiêu đề); nhấn **Alt+1…9** hoặc gõ **#2** vào ô chat để mở email số 2; danh sách thư mở ra hiển thị đúng số trích dẫn trên từng dòng.
- Khi mở trợ lý AI trên màn hình hẹp, danh sách thư thu hẹp trước để khung đọc thư không bị ép quá nhỏ.

## 0.4.4
- **Gom nhóm bằng model NPU** không còn báo “The model didn't return a valid grouping”: với model nhỏ (NPU/local), Nelo tự gom thư theo người gửi/tên miền (bỏ qua Gmail/Outlook… và tên miền của chính bạn — gom theo địa chỉ), model chỉ đặt tên và mô tả từng nhóm bằng một dòng ngắn; luật đề xuất là “Từ — tên miền/địa chỉ”. Model lớn vẫn dùng JSON nhưng đọc khoan dung hơn (id dạng chuỗi, dấu phẩy thừa, code fence).
- **Gợi ý nhóm cho thư mới** với model nhỏ: thư của người gửi đã có thư trong một nhóm được gợi ý ngay vào nhóm đó (không cần AI); phần còn lại hỏi model theo từng lô 8 thư, mỗi thư một dòng.
- **Luật lọc áp dụng chắc chắn cho thư mới**: sau mỗi lần đồng bộ, Nelo chạy lại các luật cho thư vừa đến theo đúng cách của nút “Run all on existing mail” (tiêu đề + nội dung + tệp đính kèm thật). Thư mới luôn được tải nội dung (kể cả thư lớn tới 40 MB) để luật theo nội dung hoạt động; một thư lỗi không còn làm dừng việc tải nội dung/áp luật cho các thư khác; lỗi ở bước đồng bộ cờ không làm tải lại trùng thư.
- NPU Intel: prompt được giới hạn trong độ dài OpenVINO đã biên dịch.

## 0.4.3
- **Hỗ trợ NPU Intel Core Ultra** (Meteor Lake, Lunar Lake, Arrow Lake, Panther Lake): Nelo tự nhận diện CPU/NPU, cài **OpenVINO Model Server** (thay cho FastFlowLM trên máy AMD) và đưa ra danh sách model từ bộ sưu tập *OpenVINO LLMs optimized for NPU* (Phi-3.5-mini, Qwen3, DeepSeek-R1-Distill…). Lần nạp đầu tiên OpenVINO biên dịch model cho NPU nên có thể mất vài phút. Chỉ hỗ trợ NPU của AMD và Intel.
- **Cài / cập nhật driver NPU ngay trong Nelo**: Intel — tải bộ cài chính thức từ Intel (kiểm tra SHA-256 nếu trang có công bố) rồi chạy với quyền quản trị; AMD — tìm và cài driver NPU qua Windows Update (AMD yêu cầu đăng nhập để tải trực tiếp); nếu không có, Nelo mở trang tải của hãng. Sửa lỗi nhận diện nhầm “Microsoft Input Configuration Device” thành NPU.
- **Chọn thư mục lưu runtime và model** (Cài đặt → AI assistant): xem đường dẫn + dung lượng, đổi thư mục (chuyển file cũ sang hoặc bắt đầu trống), quay về mặc định, mở trong Explorer.
- Máy **không có NPU**: ẩn toàn bộ phần AI trên máy, không cho tải runtime hay model.
- **Gợi ý nhóm cho thư mới khi NPU đang ngủ**: thư mới được đưa vào hàng chờ (không đánh thức NPU, không làm chậm thông báo thư mới); khi NPU khởi động lại, Nelo xử lý hàng chờ và hiện popup “Đã có gợi ý phân loại” ở góc trên.
- Sửa hiệu ứng chuyển trang (mở Cài đặt khi bảng AI đang mở làm hai trang hiện cạnh nhau): các trang giờ chồng lên nhau và mờ dần.
- **Mặc định có sẵn**: Google OAuth client của Nelo (đăng nhập Gmail một chạm) và **Gemini (built-in)** tự chọn model Gemini Flash mới nhất. Khoá được nhúng lúc build từ `secrets.local.env` (không nằm trong mã nguồn; xem `secrets.example.env`).

## 0.4.2
- Danh sách model NPU lấy từ bộ sưu tập chính thức **FastFlowLM flm-models-XDNA2** trên Hugging Face; tự bỏ model không chạy được trên NPU của máy, model chỉ dành cho ảnh (VL/OCR/y tế) và model chuyên dụng (dịch, chép lời, phân loại).
- Nút mở trang model trỏ tới trang Hugging Face của model (trước đây trỏ nhầm tới đường dẫn tải nội bộ → 404).
- Nelo đọc thông tin NPU khi khởi động: tên thiết bị, driver, thế hệ (XDNA 1/2), RAM; báo rõ khi driver cũ hoặc NPU không được hỗ trợ.
- Cài runtime có chỉ báo từng bước (tìm bản mới → tải → cài tệp n/430 → hoàn tất), vẫn hiển thị khi rời trang Cài đặt; trạng thái “đã cài” cập nhật ngay.
- Tiến trình tải model theo byte chính xác hơn (đúng thư mục model của FastFlowLM).

## 0.4.1
- **Streaming**: câu trả lời hiện dần; khối **“Thinking…”** chạy chữ theo thời gian thực (ChatGPT/Grok/local/NPU, Gemini, Claude API, Claude Code CLI), tự thu gọn thành “Thought for Ns”; nút **Stop** (với NPU còn gọi `/api/cancel` để giải phóng NPU ngay).
- **Tìm kiếm web theo thời gian thực** (nút “Web” trong khung chat): Gemini, Claude API, Claude Code tự tìm; các mô hình khác do Nelo tìm qua DuckDuckGo (mặc định), Brave Search API hoặc SearXNG. Chỉ gửi câu hỏi tới công cụ tìm kiếm, không gửi nội dung thư; nguồn được đánh số [1], [2] và bấm để mở.
- **Duyệt và tải thêm model cho FastFlowLM** từ danh sách đi kèm runtime: tìm theo tên, lọc “Nhỏ (≤ 5B)”, “Vừa với máy này” (theo RAM), “Model trò chuyện”; hiển thị dung lượng tải, RAM cần, ảnh/âm thanh/suy nghĩ/công cụ; thanh tiến trình theo byte, tốc độ và thời gian còn lại.
- Kiểm tra mô hình có đọc được ảnh không trước khi gửi; Claude Code CLI đọc ảnh qua công cụ Read.
- Kéo thả hoặc dán (Ctrl+V) tệp và ảnh vào khung chat; **OCR PDF scan** (tối đa 8 trang) bằng mô hình đọc ảnh.
- Cài kỹ năng từ **cả thư mục** (`SKILL.md` + tệp `.md` tham khảo; script không bao giờ được chạy).

## 0.4.0
- Logo mới; giao diện **tiếng Anh (mặc định)** và **tiếng Việt** (Cài đặt → Language).
- Tìm kiếm: nhiều từ = **từ1 OR từ2**; `"cụm từ"` trong ngoặc kép = đúng thứ tự, liền nhau; tô sáng từ khoá trong danh sách và trong nội dung thư.
- Khung đọc mới: nội dung thư nằm trong khung cố định tự cuộn (hết lỗi cuộn lồng nhau, hết nháy khi đồng bộ), nút thao tác cố định ở dưới, **phóng to/thu nhỏ** (Ctrl + lăn chuột).
- Xem trước tệp đính kèm ngay trong app (ảnh, PDF, video, văn bản, Word/Excel/PowerPoint dạng chữ), có zoom, tải xuống, mở bằng ứng dụng khác.
- **Hẹn giờ gửi** và **hoàn tác gửi** (15–300 giây, mặc định 60 giây; khung đếm ngược ở góc với “Gửi ngay” / “Huỷ”).
- Nhóm hai cấp: **nhóm chung** (mọi hộp thư) và **nhóm riêng** cho từng hộp thư; thư trong nhóm luôn ẩn khỏi Hộp thư đến; sau khi tạo quy tắc, Nelo hỏi có áp dụng cho thư cũ không.
- Chọn nhiều thư + **Chọn tất cả**, thanh thao tác hàng loạt.
- Google: chọn **IMAP hoặc POP3** trước khi đăng nhập; với POP3 hỏi có dùng chế độ `recent:` không.
- Trợ lý AI v2: đọc **toàn bộ nội dung thư và tệp đính kèm** (PDF, HTML, Office, ảnh với mô hình đọc được ảnh); chọn mô hình ngay trong khung chat; bật/tắt suy nghĩ; đính kèm tệp; lưu và mở lại cuộc trò chuyện; khoảng ngày tuỳ chọn; “Viết lại” hỏi ngay trong khung chat; gom nhóm theo nội dung kèm **đề xuất quy tắc**; tạo chữ ký bằng AI; **kỹ năng (skills)**; AI gợi ý nhóm cho thư mới.
- **AI trên NPU** (Ryzen AI 300+): tải runtime FastFlowLM và mô hình (Qwen3.5, Gemma 4, Phi-4-mini, Llama 3.2…) có thanh tiến trình. Runtime chỉ chạy khi bạn dùng AI, tự giải phóng mô hình khi rảnh / khi thu xuống khay / khi thoát (kể cả khi crash, nhờ Windows Job Object), chế độ tiết kiệm điện khi dùng pin. Powered by [FastFlowLM](https://github.com/FastFlowLM/FastFlowLM).
- Hiệu ứng chuyển cảnh cho trang, tab cài đặt, hộp thoại, menu, bảng AI, thông báo.
