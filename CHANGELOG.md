# Changelog

All notable changes to Hermio. Versions before 0.5.0 were released under the name **Nelo**
(entries kept in their original Vietnamese).

## 0.8.0

- **New-mail popup by Hermio**: bold subject, From / To right under it (half the gap Windows uses), the time on the right; stays 7 s (paused while hovered), click opens the email. It never takes the focus, respects Do not disturb, plays the mail sound (can be turned off), and a silent copy still lands in the Windows notification centre. Settings → Notifications can switch back to plain Windows notifications.
- **Email background** setting (Appearance): White (default — mail as the sender designed it), Dark (formatted mail is darkened too, pictures keep their colours) or System (follows Windows). Hermio no longer changes it on its own.
- Drop-down lists (e.g. From in the composer) were light text on a light list in dark mode.
- POP3 (Gmail): mail you send no longer shows up in that mailbox's Inbox with a notification — it goes to Sent (or is matched with Hermio's own copy there). Mail put in the Inbox by earlier versions is moved.
- Pictures in the message or signature are sent as inline attachments instead of data: links, so Gmail (web and phone) and other apps show them.
- **Writing assistant**:
  - A round button next to "AI Assistant" while you write: a sweeping ring while it checks, then the number of suggestions ringed in the colour of the most serious kind (red/blue/green/purple); "!" with the reason if checking failed.
  - Its panel opens in the AI assistant's place (the email narrows; the two share the spot).
  - Check page (text on screen) / Whole document, a Scanning ⇄ Stopped switch (stopped = no checks at all), Pause / Resume with "Scanning the page…" / "Scanning the document… n/total", Check as you type in the panel.
  - Typing checks only what's on screen; unchanged paragraphs are never sent again; no checks while scrolling (1 s after it stops); an edited paragraph keeps its earlier suggestions until the new ones arrive.
  - Clicking a suggestion in the panel highlights its text in the email and scrolls to it.
  - The hover card doesn't appear when a menu, dialog or popup covers the text.
  - Right-click in the email: Meaning, Translate, Ask AI, Cut, Copy, Paste.
  - If the chosen AI provider isn't connected, the first connected one is used.
- Smoother panels: the AI assistant panel slides with transform/opacity only instead of re-laying out the mail area on every frame.

## 0.7.5

- Notifications show the Hermio icon again. Since 0.7.3 the icon file Windows was told to use sat in the data folder, which moved (and may be synced by OneDrive), so Windows showed no icon. It now lives in local AppData under a new name.

## 0.7.4

- New-mail notifications use one layout: the subject is the title, then From / To with the time on the right. The attribution layout (which made Windows add "Hermio / New notification") and the layout setting are gone.
- A full backup, restore or copy from an older copy keeps running with its progress when you leave Settings: a small progress card shows it from anywhere (click it to go back).

## 0.7.3

- All of Hermio's data now lives in `Documents\Hermio_Emails` (created if missing), for the installed and the portable copy alike. On the first start, the existing data (from `%APPDATA%\app.hermio.mail` or a portable copy's `data` folder) is moved there — renamed when on the same drive, otherwise copied and the old folder removed only after everything arrived — and the paths stored in the database are updated.

## 0.7.2

- Deleting a draft works reliably: opening a draft (or a reply) no longer saves a new copy by itself — previously the copy replaced the draft seconds after it was opened, so the one being deleted was already gone and the copy stayed. Deleting or moving the draft that is open in the editor now closes the editor without saving it again.
- Gmail: replaced draft versions go to Trash and are purged, instead of piling up in All Mail (on Gmail an IMAP expunge only removes the label).
- Clicking the draft already open in the editor keeps the editor as it is.

## 0.7.1

- Drafts save as you write: every change is stored in the mailbox's Drafts folder a few seconds after you stop typing (one copy per message — older versions are replaced). The composer shows "Saving…" / "Saved to Drafts".
- Saving a draft is faster and the Drafts folder updates right away (the save and the folder refresh share one connection); "Save draft" closes the editor immediately.
- Folders refresh on their own after sending, moving or saving: Sent, Drafts and the destination folder are brought up to date at once, and a sync requested while another is running is no longer dropped.
- Opening a draft shows the editor with what you were writing, instead of a "Continue editing" button.
- Favorites: unified Sent and Drafts across all signed-in mailboxes.
- Writing assistant checks continuously while you write or reply (also right when a reply or draft opens); the hover card always offers Accept / Dismiss / Add to dictionary.
- New-mail notifications: no more empty band at the top (the invisible-title layout was removed).

## 0.7.0 — Writing assistant

Themisio's Grammarly-style assistant, in the composer:

- **Check writing** (Hermio AI ▸ Check writing, Ctrl+Alt+G, or the round button at the bottom
  right of the composer): every paragraph you wrote — not the quoted message or the signature — is
  checked for **Correctness, Clarity, Engagement and Delivery** against your writing goals.
  Issues are underlined (red, blue, green, purple); hover one for a card with the fix (click the
  green button to accept), why, *Dismiss* and *Add to dictionary*. Fixes are normal edits, so
  Ctrl+Z undoes them. Only changed paragraphs are sent again; other suggestions in a paragraph
  survive accepting one.
- **Panel**: overall score (0–100), counts per criterion (click to filter), tone, and tabs for
  Suggestions, **Goals** (domain, formality, audience, intent, language variety, personal
  dictionary), **Write** (more formal, friendlier, concise, expand, simplify, improve, ideas,
  summarise, or your own prompt — insert or replace) and **Report** (words, sentences, reading
  time, most used words, your progress).
- **Hermio AI popup — Ask AI / Translate / Meaning**: Ctrl+Alt+K / T / D in the composer (or
  the Hermio AI menu); in emails, double-click a word for its meaning or select text and press
  Ctrl+Alt+T / D. Translate uses Google Translate or any connected model (Vietnamese ↔ English
  or 16 other languages); Meaning uses the Free Dictionary for English words and the model
  otherwise. Replace / Insert / Copy.
- Fixed compared to Themisio: the popup opens **at the pointer** and no longer closes by itself
  0.35 s after opening — it closes with Esc, a click outside, or after you have been on it and
  moved away. Tabs and buttons are readable in both themes (active tab in the accent colour with
  white text, outlined secondary buttons, a solid green fix button).
- Settings ▸ AI assistant ▸ **Writing assistant**: model, *Check as you type* (off by default —
  every check is a request), *Double-click a word to see its meaning*, standing instructions.

## 0.6.4

- New-mail notifications: the subject is bold; From and To sit on consecutive lines with no
  gap; the gap under the subject is about half what it was. Windows 11 draws a toast's title
  in regular weight with a fixed gap, so the subject is now bold content text, and the title
  Windows requires is either invisible (layout 1) or a small "Hermio" line at the bottom
  (layout 2). *Send a test notification* shows both; Settings → Mail → *Notification layout*
  picks one.

## 0.6.3

- **AI Assistant** button (was "Assistant"). Opening the panel no longer re-runs the last AI
  action: an action started from a menu ("AI: summarize…") used to run again every time the
  panel was reopened, spending tokens. It now runs once.
- Notifications: the subject alone is the bold title (no "Subject:" prefix); the rows below have
  bold **From:** / **To:** labels with the addresses in regular weight, each row on its own line
  with a little space between them, and the time on the right of the From row.
- Notifications follow the interface language (Từ: / Đến:, 24-hour time, "(+n thư khác)").
- The remaining untranslated labels in the app (model, email, calendar navigation, slogan) are
  now in Vietnamese too.

## 0.6.2

- **Unread count on the taskbar icon**: the total unread mail in the inboxes of every
  signed-in mailbox, as a red badge on Hermio's taskbar button (Windows overlay icon) and on
  the tray icon, whose tooltip also gives the number. Settings → Mail → *Unread count on the
  taskbar icon*: Number / Dot / Off.
- **Hide the navigation rail** (Mail / Calendar / Settings) with the new *Hide* button: it folds
  away into a thin line at the window's left edge; hovering the line slides the rail back out
  for a moment, and *Pin* keeps it open again.
- The divider between the folder pane and the mail list no longer draws a stray line next to
  the panel's own edge; it shows a rounded handle only while you hover or drag it.

## 0.6.1

- Notifications no longer start with "Hermio / New notification". Windows inserts those lines
  when a toast has no title of its own; the subject is now the title, with From / To below and
  the time on the right. Calendar reminders likewise show the event title as their title.

## 0.6.0

**Notifications**
- New layout: `Subject: …` with the time it was sent in the top-right corner, then
  `From: <sender address>` and `To: <your address>` (other recipients are left out).
  Built as Windows toast XML (WinRT) instead of plain text.

**Calendar from email**
- Opening an email that carries a calendar file (.ics) asks whether to add the event. For an
  invitation it asks **Yes / Maybe / No**: the answer is sent to the organizer (iCalendar REPLY)
  and Yes or Maybe also add the event to your calendar (the mailbox's own Google calendar when
  there is one, otherwise the local calendar). Times use the invitation's time zone.
- The prompt is asked once per email. The event bar in the email keeps the Yes / Maybe / No (or
  Add to calendar) buttons and shows what you answered. Events already in the calendar
  aren't added twice.
- A popup confirms what happened, with a button to open the calendar on that event.

**Calendar reminders**
- 12 h, 6 h, 4 h, 2 h, 1 h, 30, 15 and 10 minutes before each timed event: a Windows reminder
  (stays until dismissed) and a card in the window. Every reminder has **Skip remaining
  reminders**, which silences the rest for that event. Steps missed while Hermio was closed
  collapse into one reminder. *Open* jumps to the event; *Join* opens its Meet link.
- Settings → Mail → *Remind me before calendar events* turns them off.

**Feedback everywhere**
- A thin progress bar runs along the top of the window whenever you wait more than 0.3 s on
  anything (saving, moving, importing, backups…); downloads and installs keep their own
  progress bars.
- Every backend failure now shows a popup (once, even when the screen also reports it).
- Confirmations were added where they were missing: spaces, groups, rules turned on/off,
  removing mail from a group, web search settings, removing a mailbox, marking read/unread,
  flags. Pressing Sync (or F5) reports when the mailboxes are up to date, or which failed.

**Fixes**
- The reading pane could collapse to a narrow strip since 0.5.2 (a style clash in the new
  cross-fade); the multi-selection counter looked wrong for the same reason.

## 0.5.3

- New-mail notifications now read **New email** with three lines:
  `From: Name <address>`, `To: …` (your address first, then up to two other recipients and
  "+n"; your own address when you were Bcc'd) and `Subject: …`. Long From/To lines are shortened
  so the subject always fits. The summary for a burst follows the same layout.

## 0.5.2

- **Clicking a new-mail notification opens that email.** Hermio comes back from the tray (or
  from being minimised), switches to the inbox the mail arrived in, selects the email and scrolls
  to it. Mail that went straight into a hidden group opens on its own. The summary notification
  for a burst opens the newest of them. Windows only reports the click while the toast is on
  screen, not later from the notification centre.
- **Smooth reading pane.** Picking another email fades the current one out and the new one in
  (with a slight rise), instead of blanking the pane, flashing a loading skeleton and popping the
  new email in. The skeleton only appears when a body takes longer than ~0.3 s, and it now uses
  the theme's colours (it was light grey in dark mode). Fast arrow-key browsing stays responsive.

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
