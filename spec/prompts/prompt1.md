carikan metode metode untuk misal kayak opencode, pi, dll untuk bisa save token
jadi saya ingin bikin cheap harness saya sendiri 
di repo: https://github.com/mcpe500/husk-cli.git

User ingin membuat CLI harness sendiri di repo husk-cli yang bisa menghemat token, mirip dengan opencode, pi, dll. Saya perlu mencari metode-metode untuk save token. Saya akan mencari informasi tentang cara menghemat token pada CLI AI coding agents, termasuk strategi seperti prompt caching, context management, tool output truncation, dll. Saya juga akan membuka repo husk-cli untuk melihat strukturnya. Saya akan melakukan pencarian dalam bahasa Indonesia dan Inggris.
Found 71 web pages
Read 1 page
GitHub - mcpe500/husk-cli · GitHub

Setelah membuka repo, saya melihat bahwa husk-cli adalah CLI multi-agent berbasis Rust dengan TUI. Sekarang saya perlu mencari metode-metode untuk menghemat token. Hasil pencarian menunjukkan beberapa strategi: kompresi output, pemangkasan konteks, caching prompt, dan lain-lain. Saya akan membuka beberapa hasil yang relevan untuk mendapatkan detail lebih lanjut.
Read 8 pages
如何在CLI工作流程中降低Agent的Token成本 - WaveSpeed Blog - 浏览中文版本
GitHub - ppgranger/token-saver: Content-aware output compression for AI coding assistants. 36 specialized processors cut CLI output tokens by 60-99% (git, pytest, npm, terraform, kubectl, docker, and more) without losing errors, diffs, or stack traces. · GitHub - GitHub - ppgranger/token-saver: Content-aware output compression for AI coding assistants. 36 specialized processors cut CLI out...
Cara Mengurangi Biaya Token Agen dari CLI (Panduan 2026) - Agen pengkodean CLI merasa bebas hingga tagihan tiba
GitHub - Zulf1qar/opencode-dynamic-context-pruning: Dynamic context pruning plugin for OpenCode - intelligently manages conversation context to optimize token usage · GitHub - GitHub - Zulf1qar/opencode-dynamic-context-pruning: Dynamic context pruning plugin for OpenCode - intelligently manages conversa...
GitHub - voska/pi-rtk: RTK (Rust Token Killer) integration for Pi — reduces LLM token consumption by 60-90% · GitHub - GitHub - voska/pi-rtk: RTK (Rust Token Killer) integration for Pi — reduces LLM token consumption by 60-90% · GitHub
View All

Saya sudah mengumpulkan banyak informasi tentang metode penghematan token. Sekarang saya perlu merangkumnya dalam jawaban yang terstruktur. Saya akan membahas strategi-strategi utama seperti kompresi output, pemangkasan konteks dinamis, caching prompt, manajemen skill, dan arsitektur harness yang efisien token. Saya juga akan menyertakan contoh implementasi dan referensi ke repo husk-cli. Saya akan menggunakan bahasa Indonesia sesuai permintaan user.

Berikut adalah rangkuman metode dan strategi yang bisa Anda terapkan untuk membangun cheap token harness di husk-cli. Saya bagi menjadi beberapa lapisan berdasarkan di mana penghematan terjadi — dari yang paling berdampak hingga yang lebih teknis.
1. Kompresi Output CLI (Dampak Terbesar: 60–99%)

Ini adalah lapisan pertama dan paling efektif. Prinsipnya: jangan pernah biarkan output mentah CLI masuk ke context window. Output terminal dirancang untuk dibaca manusia, bukan LLM — penuh progress bar, ANSI color codes, dependency trees, dan boilerplate.

Strategi implementasi:

    Source-side filtering — Filter output di sisi command, bukan setelahnya. Contoh: jalankan npm test --silent atau cargo test 2>&1 | grep -E '(error|FAILED|panicked)' alih-alih menangkap seluruh output.

    Content-aware processors — Buat modul khusus per command type yang tahu apa yang harus dipertahankan dan dibuang. token-saver misalnya memiliki 36 processor untuk git, pytest, cargo, docker, dll., dan berhasil memangkas token hingga 99% pada npm install.

    Pre-execution rewrite — Sebelum command dijalankan oleh agent, rewrite menjadi versi yang sudah terkompresi. RTK (Rust Token Killer) melakukan ini dengan intercepting command di antara shell dan agent, menghasilkan penghematan 60–90%.

Di husk-cli (yang sudah berbasis Rust), ini bisa diimplementasikan sebagai module output_compressor yang memiliki registry processor per command pattern. Setiap processor menerima raw output, dan mengembalikan compressed version yang hanya berisi errors, diffs, dan actionable data.
2. Dynamic Context Pruning (Dampak: 40–90%)

Setelah output masuk ke context, ia akan terus menumpuk di history percakapan. Setiap turn berikutnya, model harus memproses ulang semua output lama. Solusinya adalah pruning dinamis.

Pendekatan yang terbukti:

    Compress tool — Expose sebuah tool ke model yang bisa dipanggil untuk mengganti rentang percakapan yang sudah usang dengan ringkasan teknis high-fidelity. Di OpenCode, DCP (Dynamic Context Pruning) menggunakan dua mode: range (kompres span kontinu) dan message (kompres per-message).

    Deduplikasi — Identifikasi tool call berulang (tool + arguments sama) dan simpan hanya output terbaru. Agent yang membaca package.json empat kali dalam satu session hanya membuang token.

    Purge errors — Hapus input dari tool call yang error setelah beberapa turn (default: 4). Pesan error tetap dipertahankan, hanya konten input besar yang dihapus.

    Protected content — Pastikan output dari subagents, skills, dan file pattern tertentu tidak pernah dipruning, karena informasi ini kritikal.

Untuk husk-cli, karena sudah menggunakan multi-agent DAG, pruning harus dilakukan secara cerdas: output dari Validation Agent yang sudah selesai bisa di-prune, tapi output dari Dev Agent yang sedang looping harus dipertahankan.
3. Prompt Caching & System Prompt Ordering (Dampak: 40–88% cache hit)

LLM providers (Anthropic, OpenAI, DeepSeek) menggunakan prefix-match KV caching: jika prompt dimulai dengan byte yang sama seperti request sebelumnya, computed KV states di-reuse dan cache hits hampir gratis.

Masalah utamanya: Kebanyakan CLI agent menaruh konten dinamis di depan system prompt — handoff block, current date, memory injection — sehingga cache prefix selalu busted.

Solusinya:

    Reorder system prompt — Taruh block stabil (CLAUDE.md, agent definitions, MCP tools, skill definitions) di awal, dan block dinamis (currentDate, handoff, memory) di akhir. Ini meningkatkan cache hit rate 40–88%.

    Split static/dynamic content — Gunakan --system-prompt terpisah untuk konten statis vs dinamis, sehingga prefix yang stabil bisa di-cache di antara 20+ runs per hari. Cached input tokens 90% lebih murah di Anthropic API.

    Stable block isolation — Pastikan CLAUDE.md atau file memori tidak berubah terlalu sering. Jika berubah setiap session, seluruh cache di belakangnya akan invalid.

Di husk-cli, ini bisa diimplementasikan sebagai prompt_assembler yang secara konsisten menaruh bagian stabil di depan, dan memberi hash prefix yang bisa diverifikasi untuk cache reuse.
4. Manajemen Skills & Tools (Dampak: ~90% context tokens)

Agent CLI modern memuat semua installed skills ke context window setiap session. Setiap skill memakan ~45 token hanya untuk nama dan deskripsi. Dengan 200+ skills, ini bisa mencapai 9.000+ token terbuang per session.

Solusinya:

    Skill freezing — Pindahkan skill low-frequency dari direktori skills/ ke skills-cold/, dan install satu skill-router yang berisi index kategorisasi. Agent melihat daftar isi, bukan seluruh buku. Ini memangkas ~90% context tokens.

    Progressive disclosure — Hanya load skill saat benar-benar dibutuhkan. Skill yang di-freeze bisa diaktifkan sementara via symlink saat diperlukan.

    CLI + Skill instead of MCP — Ganti MCP tools dengan local CLI commands + Skill. MCP tools cenderung memuat semua deskripsi tool ke system prompt, sementara CLI commands hanya dimuat saat dipanggil.

5. Model Routing & Cost Measurement (Dampak: Variabel, tapi signifikan)

Tidak semua sub-task memerlukan model yang sama mahal. Routing sub-task ke model yang lebih murah bisa menghemat biaya secara drastis tanpa mengorbankan kualitas.

Strategi:

    Model routing — Gunakan model kecil untuk sub-task rutin (grep, file reading, formatting), dan model besar hanya untuk reasoning kompleks. Ini disebut sebagai salah satu taktik utama dalam pengurangan biaya token.

    Measure cost per run — Tanpa pengukuran, Anda tidak tahu apa yang benar-benar menghemat. Instrumentasi setiap request dengan token counter, dan laporkan cost per sub-task. Ini memungkinkan Anda mengidentifikasi di mana token paling banyak terbuang.

    Batched execution — Jika ada beberapa command yang perlu dijalankan, batch dalam satu tool call daripada memanggil satu per satu. Setiap tool call memiliki overhead token untuk formatting dan parsing.

Di husk-cli, karena sudah ada Execution Graph dengan subagents, routing bisa diimplementasikan sebagai model_selector yang memilih model berdasarkan AgentType dan TaskComplexity.
6. Arsitektur Harness yang Efisien Token

Beberapa prinsip arsitektural yang bisa Anda terapkan langsung:

    Zero-dependency kernel — Semakin sedikit dependency, semakin kecil overhead. ringzero misalnya membangun agent loop, providers, tools, dan sessions hanya dengan Node builtins.

    Minimal agent loop — Agent loop yang efisien tidak perlu framework berat. GenericAgent memiliki core codebase hanya ~3.300 baris dengan agent loop 92 baris.

    Streaming vs. buffering — Untuk output yang besar, gunakan streaming dengan line-by-line filtering daripada menunggu seluruh output selesai baru difilter. Ini mengurangi memory footprint dan memungkinkan early termination.

    Index-backed flows — Untuk output yang besar dan perlu searchable, simpan di local index (FTS5 + BM25) dan expose search tool ke agent. Hanya hasil search yang masuk context, bukan seluruh output.

Ringkasan Prioritas Implementasi untuk husk-cli
Prioritas	Lapisan	Estimasi Penghematan	Effort
1	Output compression (source-side filtering)	60–99%	Medium
2	Prompt cache ordering (stable blocks first)	40–88% cache hit	Low
3	Dynamic context pruning	40–90%	High
4	Skill freezing + progressive disclosure	~90%	Low
5	Model routing per sub-agent	Variabel	Medium
6	Index-backed large output	~93%	High

Mulailah dari lapisan 1 dan 2 karena keduanya memberikan penghematan terbesar dengan effort paling rendah. Lapisan 1 bisa diimplementasikan sebagai module output_filter yang di-register per command pattern. Lapisan 2 cukup dengan mengubah urutan system prompt assembly — taruh CLAUDE.md, agent definitions, dan tool definitions di depan, dan current date / handoff memory di belakang.

Setelah itu, baru masuk ke dynamic context pruning yang memerlukan tracking state percakapan dan integrasi dengan Execution Graph yang sudah ada di husk-cli.
nah kan hal hal kayak caveman, dll 
apakah ada lagi?
terus gimana kalau saya ingin meng embed semua model kecil?
mosal mini CPM? mini CPM 6?

User bertanya: "nah kan hal hal kayak caveman, dll apakah ada lagi? terus gimana kalau saya ingin meng embed semua model kecil? mosal mini CPM? mini CPM 6?"

Saya perlu mencari informasi tentang "caveman" dalam konteks AI/LLM, mungkin terkait dengan metode penghematan token atau arsitektur agent. Mungkin "caveman" adalah istilah untuk pendekatan minimalis. Saya juga perlu mencari tentang "mini CPM" atau "MiniCPM" (model kecil dari OpenBMB). User ingin meng-embed semua model kecil, mungkin untuk digunakan sebagai sub-agent atau routing. Saya perlu mencari metode embedding model kecil ke dalam harness.

Saya akan mencari:

    "caveman AI agent token saving" atau "caveman LLM"

    "MiniCPM" dan "MiniCPM 6" (mungkin MiniCPM-V atau versi terbaru)

    "embed small model in agent harness" atau "local model routing agent"

    Mungkin juga tentang "prompt compression" atau "context compression" selain caveman.

Saya akan melakukan pencarian paralel.
Found 50 web pages

Hasil pencarian menunjukkan "caveman" adalah skill untuk Claude Code yang mengurangi token output dengan gaya bicara singkat. Ada juga proyek lain seperti "context-guru", "thincontext", "SWE-Pruner", dll. Untuk model kecil, ada MiniCPM-o 2.6 dan MiniCPM-V 4.6. Ada juga harness seperti "neo", "harnage", "agent-mini" yang dirancang untuk model kecil. Saya perlu membuka beberapa tautan untuk detail lebih lanjut.
Read 5 pages
GitHub - JuliusBrussee/caveman: 🪨 why use many token when few token do trick — Claude Code skill that cuts 65% of tokens by talking like caveman · GitHub
GitHub - AramDevops/neo: An open-source agent harness that makes small models do big work. · GitHub
@nxisai/agent-harness - ⚠️
GitHub - marios-gasparis/petasos-routing-agent: Token-efficient hybrid routing agent for AMD Developer Hackathon ACT II (Track 1) — routes between local Qwen2.5-3B and Fireworks AI models to minimize cost while passing an LLM-judge accuracy gate. · GitHub - GitHub - marios-gasparis/petasos-routing-agent: Token-efficient hybrid routing agent for AMD Developer Hackathon ACT II (Track 1...
context-guru

Saya perlu mencari lebih banyak tentang "caveman" dan metode lain. User juga bertanya tentang "mosal mini CPM? mini CPM 6?" Mungkin maksudnya "MiniCPM" versi 6? Saya perlu mencari "MiniCPM 6" atau "MiniCPM-V 4.6" dan bagaimana meng-embed model kecil ke dalam harness. Saya juga perlu mencari "small language model embedding agent harness" atau "local model integration agent". Saya akan melakukan pencarian tambahan.
Found 48 web pages

Saya akan merangkum jawaban dengan dua bagian utama: (1) metode-metode penghematan token selain caveman, dan (2) cara meng-embed model kecil seperti MiniCPM ke dalam harness. Saya akan merujuk pada hasil pencarian yang relevan.

Selain Caveman (yang memangkas output gaya bicara), ada beberapa kelas metode lain yang bekerja di lapisan berbeda. Untuk rencana Anda meng-embed model kecil seperti MiniCPM, pendekatannya justru sangat selaras dengan arsitektur routing dan local-first harness.
🪨 Metode Lain di Luar Caveman

Caveman itu "small rock" — ia hanya mengubah gaya output agent agar tidak bertele-tele. Untuk penghematan yang lebih dalam, Anda perlu "big rock" yang menyentuh input, context, dan tool output.

1. Shell/Output Filter (RTK — Rust Token Killer)
RTK adalah hook di level shell yang memfilter output command Bash sebelum masuk ke agent. Ia tidak menyentuh gaya bicara model, tapi langsung memangkas output git diff, npm install, cargo test, dll. Benchmark menunjukkan RTK memangkas biaya 9% dan reward-neutral (86% task solved, sama dengan baseline), tanpa latency tambahan. RTK adalah pelengkap Caveman: RTK mengecilkan yang dibaca agent, Caveman mengecilkan yang ditulis agent.

2. Context Compression Proxy (context-guru)
Ini adalah drop-in proxy yang beroperasi di array messages sebelum request dikirim ke API. Ia melakukan deduplikasi output tool, menghapus run yang sudah usang, dan memproyeksikan file besar hanya ke baris relevan. Setiap kompresi bersifat reversible — output asli disimpan dan bisa dipanggil kembali via tool context_guru_expand. Di SWE-bench Verified, context-guru memangkas biaya 13.2% dan menyelesaikan lebih banyak task (88% vs 86% baseline) karena context lebih bersih.

3. Logica (Kompresi Struktur)
Logica adalah alternatif Caveman yang "memperbaiki struktur" bukan "memperbaiki gaya". Ia bisa diaktifkan bersamaan dengan Caveman karena keduanya bekerja di lapisan berbeda: Caveman mengompresi style, Logica mengompresi structure.

4. Deduplikasi & Sliding Window (thincontext / agent-handoff-compression)
thincontext menjalankan lima tahap kompresi berurutan pada setiap panggilan, termasuk Deduplicator (melewati konten sistem/tool yang sudah dilihat LLM di sesi yang sama) dan Chunker (hanya mengekstrak baris relevan dari konteks kode/dokumen besar). Sementara agent-handoff-compression menyediakan strategi hybrid, summary, dan sliding-window yang menggabungkan ekstraksi fakta kunci, deteksi entitas, dan identifikasi intent.

5. Skill Freezing & Progressive Disclosure
Ini bukan tentang kompresi output, tapi tentang mengurangi apa yang dimuat ke context sejak awal. Daripada memuat 200+ definisi skill ke system prompt setiap turn, simpan hanya satu skill-router yang berisi index kategorisasi. Agent melihat daftar isi, bukan seluruh buku. Ini memangkas ~90% context token dari skill definitions.
🧠 Meng-embed Model Kecil (MiniCPM, dll.)

Ini adalah pendekatan yang sangat tepat untuk "cheap harness". Prinsipnya: model kecil tidak perlu menjadi pintar — harness yang membuat mereka cukup pintar untuk tugas spesifik.

MiniCPM yang tersedia saat ini:

    MiniCPM-o 2.6 — 8B parameter total, multimodal (vision, speech, live streaming), bisa jalan di HP.

    MiniCPM-V 4.6 — 1.3B parameter, multimodal, hanya butuh 6GB RAM untuk jalan di mobile (iOS/Android), dengan context 256K.

Untuk versi 6, sejauh ini yang paling dekat adalah MiniCPM-o 2.6 dan MiniCPM-V 4.6. MiniCPM-V 4.6 adalah kandidat terkuat untuk local-first harness karena ukurannya 1.3B dan sudah tersedia dalam format GGUF untuk llama.cpp/Ollama.

Arsitektur Routing: Local-First, Escalate Surgically

Pola yang terbukti dari proyek seperti Petasos dan Neo:

    Model kecil menangani mayoritas task. Petasos menggunakan Qwen2.5-3B lokal untuk menjawab semua task terlebih dahulu (zero scored tokens), lalu hanya meng-escalate sisa yang gagal verifikasi ke model cloud. Hasilnya: 97.5%+ akurasi dengan 0–900 token cloud total, versus baseline all-remote yang menghabiskan ~30.000 token dan justru skornya lebih rendah (60%).

    Verifikasi deterministik, bukan self-confidence. Jangan percaya model kecil bilang "sudah benar". Petasos menjalankan verifiable checks: jawaban math harus menghasilkan angka yang sama dua kali, NER harus JSON valid, kode harus ast.parse. Jika gagal, baru di-escalate.

    Harness yang disiplin mengubah model lemah menjadi reliable. Neo menempatkan sequencing, verification, dan recovery di deterministic code, bukan di prompt. Model hanya mengisi konten ("tulis file ini", "pilih koordinat ini"), sedangkan urutan probe -> install -> port -> start -> verify -> open ditegakkan oleh runtime. Inilah yang membuat model murah bisa menyelesaikan task yang biasanya butuh frontier model.

Cara Embed ke husk-cli:

Untuk meng-embed MiniCPM ke harness Rust Anda:

    Provider abstraction: Buat trait LocalModelProvider yang bisa diimplementasikan oleh llama.cpp (via FFI) atau ollama (via HTTP). MiniCPM-V 4.6 sudah mendukung llama.cpp dan Ollama dengan GGUF.

    Router layer: Sebelum request dikirim ke cloud, jalankan heuristic router (regex/keyword) untuk mengklasifikasi task. Task yang bisa ditangani lokal (summarization, NER, formatting, file reading) dikirim ke MiniCPM. Task yang butuh reasoning kompleks atau verifikasi ketat di-escalate.

    Embedding untuk routing: Untuk routing yang lebih halus, gunakan model embedding kecil seperti bge-small-en-v1.5 (GGUF Q8_0) untuk mencocokkan intent user dengan deskripsi agent/skill. Ini memungkinkan routing berbasis semantic similarity tanpa memanggil LLM sama sekali.

    Verification gate: Setelah MiniCPM menjawab, jalankan cheap deterministic checks (JSON validity, regex match, dual-answer agreement) sebelum menerima hasilnya. Jika gagal, baru eskalasi.

Model kecil mana untuk apa:

    MiniCPM-V 4.6 (1.3B): Vision task, OCR ringan, summarization dokumen, routing intent — karena bisa jalan di RAM kecil.

    MiniCPM-o 2.6 (8B): Task multimodal yang lebih berat (audio, live streaming), tapi masih bisa jalan lokal.

    Qwen2.5-3B / Gemma Flash: Coding sederhana, math, sentiment — terbukti di Petasos dan Neo.

🧩 Kesimpulan untuk husk-cli

Untuk cheap harness Anda, kombinasikan:
Lapisan	Metode	Peran
Output agent	Caveman / Logica	Pangkas gaya & struktur respons
Output tool	RTK-style shell filter	Pangkas output Bash sebelum masuk context
Context	context-guru / thincontext	Deduplikasi & kompresi reversible
Skill	Skill freezing + router	Hanya muat skill yang relevan
Model	Local-first routing (Petasos/Neo)	MiniCPM/Qwen lokal menangani mayoritas, cloud hanya untuk eskalasi

Model kecil seperti MiniCPM bukan pengganti model besar — ia adalah triage layer yang mengurangi volume request ke cloud secara drastis. Dengan verifikasi deterministik dan routing yang disiplin, 1.3B parameter bisa menangani sebagian besar task rutin, dan Anda hanya membayar token cloud untuk sisa yang benar-benar membutuhkan reasoning kompleks.
pake ini: https://huggingface.co/openbmb/MiniCPM5-2B
jadi intinya model utama nya itu deepseek v4 flash 0731
terus ada model kecil mini cpm 5 tujuannya untuk semacam context management?
agent in agent? gimana?
tapi yang HARUS berpikir keras itu selalu deepseek

User wants to know about MiniCPM5-2B and how to use it for context management with DeepSeek V4 Flash as main model. Also about agent in agent architecture. I need to search for MiniCPM5-2B details, its capabilities, and context management techniques. Also DeepSeek V4 Flash 0731 info. Let me search.
Found 59 web pages

I need to open the HuggingFace page for MiniCPM5-2B, the DeepSeek V4 Flash page, and some of the agent context management pages.
Read 5 pages
add the LiteRT-LM version of MiniCPM5-2B · openbmb/MiniCPM5-2B at a063f08 - funmaker commited on
DeepSeek V4 Flash 0731 API: Pricing, Benchmarks & Docs | Together AI
Organizing Context in a Multi-Agent Harness - Organizing Context in a Multi-Agent Harness
[PDF] Three Roles, One Model: Role Orchestration at Inference Time to Close the Performance Gap Between Small and Large Agents | Semantic Scholar - Three Roles, One Model: Role Orchestration at Inference Time to Close the Performance Gap Between Small and Large Agents
U-Fold: Dynamic Intent-Aware Context Folding for User-Centric Agents - We further examine whether a stronger LLM can act as an informative folder for a weaker agent

The search results provide some information. I need to open more pages about MiniCPM5-2B, especially the HuggingFace model card, and also search for "agent in agent" architecture and context management with small models. Let me also search for "DeepSeek V4 Flash 0731 reasoning effort" to understand its capabilities.
Found 46 web pages

Now I have enough information. Let me also open the HuggingFace model card for MiniCPM5-2B to get more details.
Read 3 pages
openbmb/MiniCPM5-2B · Hugging Face - curl http://localhost:30000/v1/chat/completions \
OpenBMB Releases MiniCPM5-2B as Open On-Device SOTA Under 4B - Where China Innovation Meets the World
deepseek-docs/content/zh-cn/guides/thinking_mode.md at main · thevibeworks/deepseek-docs

Now I have enough information to answer the user. Let me structure my answer.

The user wants to know:

    About MiniCPM5-2B (from the HF link)

    How to use it for context management with DeepSeek V4 Flash as main model

    "agent in agent" architecture

    Ensuring "hard thinking" always goes to DeepSeek

I'll explain the architecture: MiniCPM5-2B as a context management sub-agent, DeepSeek V4 Flash as the main reasoning model. I'll cover:

    What MiniCPM5-2B is (2B, 131K context, tool calling, etc.)

    Why it's suitable for context management

    "Agent in agent" pattern with context isolation

    How to ensure DeepSeek handles all hard reasoning

    Implementation approach for husk-cli

I'll cite sources where appropriate.
MiniCPM5-2B: Model Kecil yang Tepat untuk Context Management

MiniCPM5-2B dari OpenBMB adalah pilihan yang sangat solid untuk peran context management sub-agent di harness Anda. Ini bukan model "kecil yang lemah" — ini adalah 2.52B dense model dengan 131K context window native, Apache-2.0 license, dan yang paling penting: native tool calling support via SGLang dengan parser XML-style yang dikonversi ke OpenAI-compatible tool_calls. Model ini menggunakan arsitektur LlamaForCausalLM standar, sehingga bisa dimuat langsung oleh vLLM, SGLang, llama.cpp, Ollama, atau LM Studio tanpa custom kernel.

Yang membuatnya cocok untuk peran context management adalah kekuatan spesifiknya: pada benchmark 34 tugas, MiniCPM5-2B unggul di area coding agents, tool use, dan long-context retrieval — tepatnya kemampuan yang dibutuhkan untuk membaca, memfilter, dan merangkum konteks besar. LiveCodeBench v6 mencapai 69.1 (vs 56.4 untuk Qwen3.5-4B), dan NoLiMa (long-context retrieval) mencapai 68.1 (vs 43.5). Ini artinya model ini benar-benar bisa "membaca" konteks panjang dengan akurat, bukan sekadar memuatnya.
Arsitektur yang Anda Gambarkan: DeepSeek sebagai Otak, MiniCPM sebagai Context Manager

Pola yang Anda deskripsikan — model utama (DeepSeek V4 Flash 0731) untuk berpikir keras, model kecil (MiniCPM5-2B) untuk mengelola konteks — adalah pola yang sudah divalidasi secara akademik. Ini disebut three-tier inference scaffolding: model yang sama atau model terpisah dipakai dalam tiga peran berbeda — summarization model yang mengompres dialog history sambil mempertahankan artifact kritis, main agent model yang bernalar di atas konteks yang sudah dikompres, dan isolated correction model yang mereview output tanpa akses ke conversation history.

Untuk kasus Anda, DeepSeek V4 Flash 0731 adalah main agent model. MiniCPM5-2B adalah summarization model — ia tidak perlu berpikir keras; ia hanya perlu membaca, memfilter, dan merangkum. DeepSeek yang memutuskan apa yang harus dilakukan, MiniCPM yang menjaga agar konteks DeepSeek tetap bersih dan ringkas.

Mengapa pola ini bekerja:

DeepSeek V4 Flash 0731 adalah MoE dengan 284B total parameters dan 13B active per token, mendukung 1M-token context window dengan hybrid attention yang menjaga biaya inference long-context tetap murah. Ia memiliki adjustable reasoning effort — low, high, max — yang bisa diatur per request. Artinya Anda bisa mengirim task ke DeepSeek dengan reasoning_effort: "max" hanya ketika benar-benar dibutuhkan, dan reasoning_effort: "low" untuk task rutin yang tidak memerlukan deliberasi mendalam.

Sementara itu, MiniCPM5-2B dengan 131K context window bisa menampung hampir seluruh percakapan panjang (kecuali yang benar-benar ekstrem) dan mengompresnya menjadi ringkasan terstruktur sebelum dikirim ke DeepSeek. Ini mengurangi jumlah token yang harus diproses DeepSeek, dan yang lebih penting: mengurangi noise dalam context DeepSeek. Konteks yang bersih membuat reasoning DeepSeek lebih akurat, bukan hanya lebih murah.
"Agent in Agent": Context Isolation, Bukan Sekadar Delegasi

Pola agent in agent di sini bukan berarti MiniCPM menjadi "agent" otonom yang bisa memanggil tool sesuka hati. Pola yang tepat adalah context isolation — MiniCPM berjalan sebagai sub-agent yang menerima tugas spesifik, bekerja di fresh context window, dan mengembalikan hasil ke main agent (DeepSeek) dalam bentuk tool result.

Ini adalah pola supervisor-worker yang sudah menjadi standar di coding harness modern. Supervisor (DeepSeek) memelihara plan dan mendelegasikan pekerjaan ke sub-agent (MiniCPM). Supervisor hanya menerima outcome dari sub-agent; intermediate reasoning MiniCPM tidak masuk ke context DeepSeek.

Dalam praktiknya untuk husk-cli:

DeepSeek adalah supervisor. Ia tidak membaca file mentah, tidak melihat seluruh output git log, tidak memuat seluruh isi repository. Ia hanya melihat ringkasan terstruktur yang dihasilkan MiniCPM. Ketika DeepSeek perlu tahu "apa yang berubah di file X antara commit A dan B", ia memanggil sub-agent MiniCPM dengan task: "baca diff file X antara commit A dan B, kembalikan ringkasan perubahan dan potensi breaking change". MiniCPM membaca diff mentah (yang mungkin 50.000 token), memprosesnya di context window-nya sendiri, dan mengembalikan ringkasan 500 token ke DeepSeek.

Ada dua mode context yang bisa Anda pilih untuk sub-agent:

    Isolated mode (default): Sub-agent spawn dengan context window kosong, hanya menerima task description dari supervisor. Ini memaksimalkan isolasi dan mencegah kontaminasi konteks. Cocok untuk task yang benar-benar independen, seperti "baca file ini dan ekstrak fungsinya".

    Forked mode: Sub-agent menerima seluruh conversation state supervisor, termasuk conversation history, lalu supervisor menambahkan directive task. Ini berguna ketika sub-agent perlu memahami konteks percakapan untuk menyelesaikan task dengan benar. Forked mode memanfaatkan prompt caching karena prefix konteks sama dengan supervisor, sehingga lebih murah dan cepat.

Untuk context management, forked mode seringkali lebih tepat karena MiniCPM perlu tahu "apa yang sedang dibicarakan" untuk bisa merangkum dengan akurat. Tapi Anda bisa memilih mode secara dinamis berdasarkan jenis task.
Memastikan "Hard Thinking" Selalu ke DeepSeek

Ini adalah bagian paling kritikal dari desain Anda. Anda ingin tidak ada penalaran kompleks yang jatuh ke MiniCPM. Ada beberapa lapisan yang bisa menegakkan aturan ini:
1. Deterministic Routing di Level Harness

Jangan biarkan model memilih model. Routing harus deterministik, diimplementasikan di kode harness, bukan di prompt. Anda bisa menggunakan heuristic sederhana:

    Task yang memerlukan reasoning kompleks → DeepSeek. Contoh: bug fixing, architectural decision, code review, planning, security analysis.

    Task yang hanya memerlukan reading/filtering/summarization → MiniCPM. Contoh: file reading, diff summarization, log filtering, context compression, entity extraction dari output tool.

Ini bisa diimplementasikan sebagai task_router di husk-cli yang menerima TaskType enum dan mengembalikan model provider yang sesuai. TaskType ditentukan oleh task definition di Execution Graph, bukan oleh model.
2. Verification Gate untuk Output MiniCPM

Jika MiniCPM menghasilkan output yang kemudian dikirim ke DeepSeek sebagai tool result, Anda bisa menambahkan deterministic verification gate sebelum hasilnya diterima. Misalnya:

    Ringkasan harus dalam format JSON valid dengan field yang sudah ditentukan.

    Ekstraksi harus mengandung semua identifier yang diminta (tidak boleh ada yang hilang).

    Ringkasan tidak boleh melebihi token budget yang ditentukan.

Jika verifikasi gagal, eskalasi ke DeepSeek untuk task yang sama. Ini memastikan bahwa ketika MiniCPM gagal, tidak ada hasil buruk yang masuk ke context DeepSeek — DeepSeek yang mengambil alih dengan reasoning penuh.
3. Reasoning Effort Mapping di DeepSeek

DeepSeek V4 Flash 0731 mendukung reasoning effort yang bisa diatur per request. Effort default adalah high, tapi bisa diatur ke low, high, atau max. Untuk memastikan "hard thinking" benar-benar keras:

    Task dengan TaskComplexity::Critical → reasoning_effort: "max"

    Task dengan TaskComplexity::High → reasoning_effort: "high"

    Task dengan TaskComplexity::Routine → reasoning_effort: "low"

Ini memungkinkan DeepSeek untuk tidak membuang token pada task rutin, dan mengalokasikan budget reasoning lebih besar pada task yang benar-benar membutuhkan.
4. Isolated Correction Model

Untuk code generation, Anda bisa menambahkan pola isolated correction model — sebuah sub-agent yang mereview output kode DeepSeek tanpa akses ke conversation history, hanya melihat kode yang dihasilkan. Ini memecah repetitive failure loops karena reviewer tidak "terjebak" dalam konteks percakapan yang mungkin salah. Reviewer ini bisa dijalankan oleh DeepSeek dengan reasoning_effort: "max" untuk memastikan review yang ketat, atau oleh MiniCPM jika review hanya perlu memeriksa sintaks dan format.
Konkretnya untuk husk-cli

Berikut adalah arsitektur yang saya sarankan:
text

┌─────────────────────────────────────────────────────┐
│                    husk-cli Kernel                   │
│  ┌──────────────┐    ┌──────────────────────────┐   │
│  │ Task Router  │───▶│  Execution Graph Engine   │   │
│  │ (deterministic)│   │  (multi-agent DAG)        │   │
│  └──────────────┘    └──────────────────────────┘   │
│         │                       │                    │
│         ▼                       ▼                    │
│  ┌──────────────┐    ┌──────────────────────────┐   │
│  │ MiniCPM      │    │  DeepSeek V4 Flash 0731  │   │
│  │ Sub-Agent    │    │  (Main Reasoning Agent)  │   │
│  │              │    │                          │   │
│  │ • File read  │    │  • Bug fixing            │   │
│  │ • Diff sum   │    │  • Architecture          │   │
│  │ • Log filter │    │  • Planning              │   │
│  │ • Context    │    │  • Code review           │   │
│  │   compression│    │  • Security analysis     │   │
│  │              │    │                          │   │
│  │ reasoning:   │    │  reasoning_effort:       │   │
│  │ low/none     │    │  low / high / max        │   │
│  └──────┬───────┘    └────────────┬─────────────┘   │
│         │                         │                  │
│         ▼                         ▼                  │
│  ┌──────────────────────────────────────────────┐   │
│  │        Context Assembly Layer                │   │
│  │  • Compressed context from MiniCPM           │   │
│  │  • Stable prefix first (prompt cache)        │   │
│  │  • Dynamic content last                      │   │
│  └──────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────┘

Alur kerjanya:

    User memberikan task ke husk-cli. Task Router mengklasifikasi task berdasarkan task definition di Execution Graph — bukan berdasarkan LLM.

    Jika task memerlukan context gathering (baca file, diff, log, search), husk-cli mendelegasikan ke MiniCPM sub-agent dalam isolated mode dengan task description spesifik. MiniCPM bekerja di context window sendiri, dan mengembalikan ringkasan terstruktur.

    Jika task memerlukan reasoning kompleks, husk-cli mengirim task ke DeepSeek dengan konteks yang sudah dikompres oleh MiniCPM, dan reasoning_effort yang sesuai dengan complexity level.

    DeepSeek bisa memanggil MiniCPM sebagai tool ketika ia perlu context tambahan — tapi ini adalah tool call eksplisit, bukan delegasi implisit. DeepSeek tetap yang memutuskan, MiniCPM tetap yang mengeksekusi context operation.

    Output MiniCPM melewati verification gate sebelum diterima. Jika gagal, eskalasi ke DeepSeek.

Mengapa Ini Bukan Sekadar "Model Kecil untuk Task Kecil"

Poin penting yang sering terlewat: MiniCPM5-2B bukan model kecil yang perlu dilindungi dari task sulit. Ia adalah model yang secara arsitektural dirancang untuk context management dan tool use. Ia unggul di long-context retrieval dan tool calling — dua kemampuan yang persis dibutuhkan untuk peran ini. Ia tidak perlu berpikir keras karena berpikir keras bukan tugasnya. Tugasnya adalah membaca, memfilter, merangkum, dan mengembalikan konteks yang bersih.

DeepSeek V4 Flash 0731, dengan 1M context window dan adjustable reasoning effort, adalah otak yang sebenarnya. Ia yang memutuskan, ia yang bernalar, ia yang menyelesaikan masalah. MiniCPM adalah asisten context yang memastikan DeepSeek tidak tenggelam dalam noise.

Pola ini sejalan dengan temuan dari U-Fold: menggunakan model besar secara sparing sebagai "folding service" untuk mendorong agen kecil yang lebih murah dalam interaksi long-horizon yang intensif tool. Dalam kasus Anda, DeepSeek adalah folding service yang digunakan secara sparing, dan MiniCPM adalah agen kecil yang menangani mayoritas operasi context.

Untuk meng-embed MiniCPM5-2B ke husk-cli, Anda bisa menggunakan llama.cpp (via FFI) atau Ollama (via HTTP) dengan format GGUF. Model ini sudah tersedia dalam GGUF, MLX, GPTQ, dan LiteRT, sehingga bisa berjalan di CPU, GPU, atau bahkan edge device. Provider abstraction di Rust bisa menggunakan trait LocalModelProvider dengan implementasi LlamaCppProvider dan OllamaProvider.
saya ingin HASILNYA itu sama atau bahkan lebih pinter kalo dibanding menggunakan deepseek only, tapi dengan ada nya mini CPM 5 2b ini bisa hasilnya itu JAUH LEBIH hemat token

jadi 
husk-cli CHEAPER THAN other harness 
husk-cli SAME OR SMARTER THAN other harness

User ingin membangun husk-cli yang lebih murah dan sama/pintar dengan harness lain, dengan MiniCPM5-2B sebagai context manager dan DeepSeek V4 Flash 0731 sebagai main reasoner. Saya perlu mencari arsitektur hybrid LLM, teknik penghematan token, hasil benchmark, dan pola routing yang sudah ada. Saya akan melakukan pencarian paralel.
Found 78 web pages

Hasil pencarian menunjukkan beberapa sumber relevan. Saya akan membuka beberapa di antaranya untuk mendapatkan informasi lebih mendetail.
Read 12 pages
Conductor Agent 记忆实战：用 SemanticMemory 实现跨会话语义检索与个性化回复 - 🔥
OpenHuman Prompt/Token-Budget 审计：从 3.7 万 token 固定前缀到分层缓存与工具披露架构 - 🔥
OpenHuman Summarizer Agent 系统提示词与运行时设计深度解析：为 Orchestrator 压缩超量工具结果的内置子代理 - 🔥
DeepSeek V4 Flash 0731 (max) - Intelligence, Performance & Price Analysis | Artificial Analysis - ••
Division-of-Thoughts: Harnessing Hybrid Language Model Synergy for Efficient On-Device Agents - Division-of-Thoughts: Harnessing Hybrid Language Model Synergy for Efficient On-Device AgentsConference: Proceedings of the ACM ...
View All

Setelah membuka beberapa sumber, saya mendapatkan informasi tentang berbagai topik. Saya perlu melakukan pencarian lebih lanjut untuk beberapa topik yang belum tercakup, seperti benchmark perbandingan harness, arsitektur hybrid, dan teknik penghematan token. Saya akan melakukan pencarian paralel.
Found 58 web pages

Sekarang saya memiliki cukup informasi untuk menyusun jawaban. Saya akan membahas arsitektur hybrid, teknik penghematan token, dan memberikan rekomendasi untuk husk-cli. Saya akan merujuk pada sumber-sumber yang relevan.

Untuk mencapai tujuan "husk-cli CHEAPER THAN other harness" dan "husk-cli SAME OR SMARTER THAN other harness", arsitektur hybrid yang Anda rancang (DeepSeek V4 Flash 0731 sebagai main reasoner + MiniCPM5-2B sebagai context manager) adalah fondasi yang sangat tepat. Kuncinya adalah membangun harness yang secara cerdas mengisolasi konteks, mengompresi hasil, dan mengalokasikan reasoning effort secara adaptif. Berikut adalah cetak biru arsitekturnya.
📊 Mengapa Harness Adalah Kunci, Bukan Model

Sebuah studi benchmark menunjukkan bahwa pilihan harness mengubah biaya token per tugas yang diselesaikan hingga 40x lipat, sementara peningkatan model hanya mengubahnya sekitar 1.0–1.3x. Ini berarti arsitektur harness Anda (bagaimana ia mengelola konteks, memanggil tool, dan mendelegasikan tugas) adalah faktor penentu biaya yang jauh lebih besar daripada model itu sendiri. Model DeepSeek V4 Flash 0731 yang Anda pilih sudah sangat efisien, dengan harga input $0.14 per juta token dan skor Intelligence Index 50, hanya terpaut satu poin dari model yang jauh lebih mahal.
🧠 Arsitektur Inti: Supervisor-Worker dengan Context Isolation

Pola yang paling tepat untuk Anda adalah Supervisor-Worker dengan Context Isolation. DeepSeek V4 Flash 0731 bertindak sebagai Supervisor yang memelihara rencana dan melakukan penalaran tingkat tinggi. MiniCPM5-2B bertindak sebagai Worker yang menangani tugas-tugas low-cognitive-density seperti membaca file, meringkas output tool, dan mengekstraksi fakta.

Mengapa ini sangat efektif untuk menghemat token?

    Kompresi Konteks Agresif: Worker beroperasi di context window-nya sendiri yang terisolasi. Ia bisa membaca 8 file dan melakukan 15 panggilan tool, tetapi hanya mengembalikan ringkasan padat 1.000–2.000 token ke Supervisor. Studi menunjukkan sub-agent dapat mengurangi token yang masuk ke konteks parent hingga 90%+.

    Konfigurasi MiniCPM5-2B: Model ini adalah pilihan yang sangat baik untuk peran ini. Dengan 2B parameter, ia mendukung native tool calling melalui backend SGLang. Yang paling penting, ia memiliki 128K context window dan dirancang untuk long-context retrieval — kemampuan persis yang dibutuhkan untuk membaca dan memfilter konteks besar.

⚙️ Tiga Pilar Teknis untuk Mencapai "Cheaper & Smarter"
1. Context Isolation: Pisahkan "Konteks Berpikir" dari "Konteks Membaca"

Ini adalah prinsip fundamental. DeepSeek Supervisor tidak boleh membaca file mentah, log, atau output tool yang besar. Ia hanya boleh melihat artifact terstruktur (ringkasan, diff, hasil ekstraksi) yang dihasilkan oleh MiniCPM Worker.

Anda bisa mengimplementasikan ini dengan dua mode konteks untuk sub-agent:
Mode	Cara Kerja	Kapan Digunakan	Dampak Token
Isolated	Worker memulai dengan context window kosong, hanya menerima deskripsi tugas spesifik dari Supervisor.	Untuk tugas yang sepenuhnya independen (misal: "baca file X dan ekstrak semua fungsi").	Isolasi maksimal, mencegah kontaminasi konteks.
Forked	Worker mewarisi seluruh riwayat percakapan Supervisor, lalu Supervisor menambahkan direktif tugas.	Untuk tugas yang membutuhkan pemahaman konteks percakapan (misal: "berdasarkan diskusi kita, ringkas perubahan di file Y").	Memanfaatkan prompt caching karena prefix konteks sama, sehingga lebih murah dan cepat.
2. Deterministic Routing: Jangan Biarkan Model Memilih Model

Aturan Anda — "yang HARUS berpikir keras itu selalu DeepSeek" — harus ditegakkan di level kode harness, bukan di prompt. Buatlah TaskRouter deterministik yang mengklasifikasikan tugas berdasarkan definisi tugas di Execution Graph, bukan berdasarkan keputusan LLM.

    Tugas untuk DeepSeek (Supervisor): Bug fixing, keputusan arsitektur, code review, perencanaan, analisis keamanan. Ini adalah tugas dengan TaskComplexity::Critical atau High.

    Tugas untuk MiniCPM (Worker): Membaca file, meringkas diff, memfilter log, kompresi konteks, ekstraksi entitas. Ini adalah tugas TaskComplexity::Routine.

Untuk klasifikasi yang lebih canggih, Anda bisa mengimplementasikan heuristik berbasis aturan (misalnya, deteksi kata kunci seperti "refactor", "debug", "analisis") yang instan dan tanpa biaya token.
3. Adaptive Reasoning Effort: Alokasikan "Kekuatan Berpikir" Secara Dinamis

DeepSeek V4 Flash 0731 mendukung adjustable reasoning effort (low, high, max). Ini adalah tuas penghematan biaya yang sangat kuat. Alih-alih selalu menggunakan max, petakan effort ke tingkat kompleksitas tugas:

    TaskComplexity::Critical → reasoning_effort: "max" (biaya token lebih tinggi, tetapi hanya untuk tugas yang benar-benar membutuhkan).

    TaskComplexity::High → reasoning_effort: "high".

    TaskComplexity::Routine → reasoning_effort: "low" (menghemat token pada tugas yang tidak memerlukan deliberasi mendalam).

Penelitian tentang Adaptive Reasoning Effort menunjukkan bahwa harness dapat secara otomatis memutuskan berapa banyak "berpikir" yang dialokasikan per langkah berdasarkan sinyal kompleksitas tugas. Ini mencegah pemborosan token pada pencarian lokal yang tidak membuahkan hasil, sekaligus menghindari overhead memanggil model besar di setiap langkah penalaran.
🏗️ Cetak Biru Arsitektur untuk husk-cli

Berikut adalah bagaimana ketiga pilar ini bersatu dalam alur kerja husk-cli:

    Input Pengguna & Klasifikasi Tugas: Tugas masuk ke TaskRouter. Router (deterministik, tanpa LLM) mengklasifikasikan tugas dan menentukan model mana yang akan menangani serta berapa reasoning_effort yang sesuai.

    Fase Context Gathering (MiniCPM Worker):

        Jika tugas memerlukan pembacaan file, diff, atau log, husk-cli mendelegasikannya ke MiniCPM sub-agent dalam mode Isolated.

        MiniCPM bekerja di context window-nya sendiri. Ia membaca semua data mentah, memprosesnya, dan mengembalikan ringkasan terstruktur (misalnya, dalam format JSON dengan field key_findings, source_references, confidence_flags).

    Verification Gate (Kode Deterministik):

        Sebelum ringkasan MiniCPM diterima, ia melewati verification gate. Gate ini memeriksa apakah ringkasan tersebut valid (format JSON benar, mengandung semua identifier yang diminta, tidak melebihi token budget).

        Jika verifikasi gagal, tugas di-eskalasi ke DeepSeek untuk ditangani dengan penalaran penuh.

    Fase Reasoning (DeepSeek Supervisor):

        DeepSeek menerima konteks yang sudah bersih dan terkompresi dari MiniCPM.

        DeepSeek melakukan penalaran dengan reasoning_effort yang telah ditentukan oleh Router.

        Jika DeepSeek membutuhkan informasi tambahan, ia dapat memanggil MiniCPM lagi sebagai tool untuk tugas context management spesifik.

    Loop Umpan Balik & Iterasi: Hasil dari DeepSeek dapat memicu siklus baru, tetapi setiap iterasi selalu dimulai dengan konteks yang bersih dan terfokus.

💎 Kesimpulan

Untuk membuat husk-cli lebih murah dan sepadan atau lebih pintar dari harness lain, fokuslah pada arsitektur, bukan hanya model. Gunakan DeepSeek V4 Flash 0731 sebagai otak yang melakukan penalaran berat, dan MiniCPM5-2B sebagai asisten context yang menjaga agar otak tersebut tidak tenggelam dalam noise. Terapkan Context Isolation untuk memisahkan konteks berpikir dari konteks membaca, Deterministic Routing untuk memastikan tugas berat selalu ditangani dengan tepat, dan Adaptive Reasoning Effort untuk mengalokasikan sumber daya secara efisien. Dengan fondasi ini, husk-cli tidak hanya akan lebih hemat token, tetapi juga berpotensi menghasilkan output yang lebih akurat karena konteks DeepSeek selalu bersih dan fokus.

model nya yang mini CPM itu akan embedded, jadi akan ada di executable nya(pastikan SUDAH ANDA EXTREME OPTIMIZED)
terus yang provider deepseek itu akan menggunakan netraruntime

Berikut adalah definisi lengkap UI/UX OpenCode CLI, yang bisa Anda jadikan cetak biru untuk `husk-cli`. Secara fundamental, OpenCode bukan sekadar CLI biasa; ia adalah sebuah **TUI (Terminal User Interface)** yang dirancang untuk pengalaman interaktif yang kaya di dalam terminal.

### 🎨 Definisi UI/UX OpenCode CLI

OpenCode mengadopsi filosofi "terminal-first" dengan TUI berbasis Ink/React yang memberikan pengalaman interaktif seperti aplikasi modern, namun tetap ringan dan dapat diotomasi.

**1. Filosofi Desain: Terminal sebagai Inti**

OpenCode memilih terminal bukan sebagai opsi cadangan, melainkan keputusan arsitektural yang disengaja. Alasannya adalah terminal merupakan **common denominator** bagi semua developer, terlepas dari OS, editor, atau bahasa pemrograman yang digunakan. Terminal juga secara natural dapat di-embed ke dalam script, CI/CD pipeline, dan Git Hooks, sehingga OpenCode bukan hanya alat interaktif, tetapi juga **infrastruktur AI yang programmable**.

**2. Tata Letak TUI: Struktur yang Jelas**

TUI OpenCode memiliki tata letak tiga zona yang bersih dan fungsional:

```
┌─────────────────────────────────────────────────────────────┐
│ [agent: build]  session: "fix-auth-bug"  tokens             │  ← Top Bar
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  Conversation (scrollable)                                  │  ← Chat Log
│                                                             │
├─────────────────────────────────────────────────────────────┤
│ > prompt input                                              │  ← Input Box
└─────────────────────────────────────────────────────────────┘
```

- **Top Bar**: Menampilkan informasi konteks secara real-time — agent yang aktif, nama session, dan penghitung token.
- **Conversation Area**: Area scrollable untuk riwayat percakapan, menampilkan dialog antara user dan agent, termasuk tool calls dan status eksekusinya.
- **Input Box**: Tempat user mengetik prompt, mendukung readline/emacs-style shortcuts.

**3. Sistem Keybinding: Leader Key & Readline**

OpenCode menggunakan **leader key** (default: `Ctrl+X`) untuk menghindari konflik dengan shortcut terminal yang ada. Contoh: `Ctrl+X` lalu `n` untuk session baru, `Ctrl+X` lalu `m` untuk browse models, `Ctrl+X` lalu `t` untuk switch theme.

Di dalam input box, OpenCode mendukung **readline/emacs-style shortcuts** yang sudah familiar bagi pengguna terminal:

| Shortcut | Action |
|---|---|
| `Ctrl+A` / `Ctrl+E` | Awal / akhir baris |
| `Ctrl+K` | Hapus sampai akhir baris |
| `Ctrl+U` | Hapus sampai awal baris |
| `Ctrl+R` | Rename session |
| `Ctrl+D` | Hapus session (saat prompt kosong) |

Navigasi utama juga didesain untuk efisiensi: **Tab** untuk cycling antar primary agents (build ↔ plan), **Esc** untuk cancel/dismiss, dan **Ctrl+C** (dua kali) untuk exit.

**4. Prompt Prefix: `@`, `!`, dan `use`**

OpenCode memiliki sistem prefix yang sangat powerful:

- **`@<path>`**: Fuzzy-search file di project dan attach kontennya ke prompt. Multiple `@` bisa digunakan dalam satu message. Contoh: `review @src/api/auth.ts and @src/api/auth.test.ts`
- **`!<command>`**: Menjalankan shell command di working directory project dan attach output-nya ke prompt. Contoh: `!git diff main...HEAD based on this diff, write a PR description`
- **`use <mcp-server>`**: Memanggil MCP tool. Contoh: `use playwright open https://example.com and take a screenshot`

**5. Slash Commands: Kontrol Penuh dari Prompt**

OpenCode menyediakan slash commands untuk kontrol session dan konfigurasi langsung dari input box:

| Command | Purpose |
|---|---|
| `/init` | Generate/update AGENTS.md |
| `/new` (alias `/clear`) | Session baru |
| `/sessions` (alias `/resume`, `/continue`) | List & switch sessions |
| `/models` | Browse models |
| `/connect` | Tambah provider |
| `/themes` | Switch theme |
| `/compact` (alias `/summarize`) | Compact session |
| `/share` / `/unshare` | Public link control |
| `/export` | Export ke markdown |
| `/undo` / `/redo` | Walk edit history |
| `/editor` | Buka `$EDITOR` |
| `/details` | Toggle tool execution detail visibility |

**6. Agent Switching: Build & Plan Modes**

OpenCode memiliki dua primary agents: **Build** (semua tools enabled) dan **Plan** (tool access dibatasi). User dapat **menekan Tab** untuk beralih antar primary agents selama session berlangsung. Ini memungkinkan user untuk memulai dengan Plan mode untuk analisis dan review, lalu beralih ke Build mode untuk implementasi.

**7. Diff View & Edit Review**

OpenCode menyediakan diff-based edit review yang terintegrasi. Setiap perubahan file yang dilakukan oleh agent dapat direview melalui diff view, dengan kemampuan untuk **accept/reject** perubahan. Ini memberikan kontrol penuh kepada user atas apa yang agent lakukan terhadap codebase mereka.

**8. Mode Scrollback: Normal vs Insert**

OpenCode memiliki konsep modal input yang terinspirasi dari Vim. User membaca percakapan sebagai **scrollback di Normal mode** dan menulis prompt di **Insert mode**. Dengan menekan **Esc**, user dapat "drop to Normal-in-terminal" untuk scroll/search scrollback menggunakan vim motions, lalu menekan **i** atau **a** untuk kembali ke Insert mode.

**9. Tema dan Kustomisasi**

OpenCode mendukung **50+ built-in themes** dan memungkinkan user untuk mendefinisikan custom theme melalui file JSON. Theme bisa di-set secara global atau per-project. OpenCode juga mendukung **single-mode themes** (hanya light atau hanya dark) dengan deteksi otomatis berdasarkan contrast background/text terminal.

**10. Arsitektur Client/Server**

OpenCode menggunakan model **client/server**: TUI adalah client sederhana untuk server yang berjalan di background. Ini memungkinkan **alternate frontends** — misalnya, `opencode run xxx` adalah semacam alternate frontend, dan ada juga **web tunnel** yang memungkinkan session diakses dari browser, desktop, atau bahkan mobile.

---

### 🏗️ Menerapkan UI/UX OpenCode ke `husk-cli`

Untuk membuat `husk-cli` memiliki UI/UX seperti OpenCode, Anda perlu mengimplementasikan komponen-komponen berikut:

**1. TUI Framework: Gunakan `ratatui` (Rust)**

Karena `husk-cli` berbasis Rust, **`ratatui`** adalah pilihan yang tepat untuk membangun TUI. Ini adalah library TUI modern untuk Rust yang menyediakan widget untuk layout, input, list, table, dan sebagainya. Anda bisa membuat layout tiga zona (top bar, conversation area, input box) dengan `ratatui` menggunakan `Layout` dan `Constraint`.

**2. Layout Tiga Zona**

Implementasikan layout seperti OpenCode:

```
┌─────────────────────────────────────────────────────────────┐
│ [agent: deepseek] session: "husk-001" tokens: 12,450        │  ← Top Bar
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  Conversation (scrollable)                                  │  ← Chat Log
│  ┌─ tool: MiniCPM context_summary ─────────────────────┐   │
│  │ ▸ running... [3 files, 12k tokens → 800 tokens]     │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│ > prompt input                                              │  ← Input Box
└─────────────────────────────────────────────────────────────┘
```

Top bar harus menampilkan informasi yang relevan dengan arsitektur Anda: agent yang aktif (DeepSeek Supervisor atau MiniCPM Worker), session ID, dan **token counter** yang menunjukkan penghematan secara real-time.

**3. Sistem Keybinding**

Implementasikan leader key (default: `Ctrl+X`) untuk aksi session-level, dan readline shortcuts di input box. Tambahkan juga **Tab** untuk cycling antar agent mode (Supervisor ↔ Worker) — ini analog dengan Build ↔ Plan mode di OpenCode.

Untuk agent switching di `husk-cli`, Anda bisa memiliki mode:

- **Supervisor Mode**: DeepSeek V4 Flash 0731 dengan `reasoning_effort` yang sesuai.
- **Worker Mode**: MiniCPM5-2B untuk context gathering dan summarization.
- **Auto Mode**: Routing deterministik yang memilih model berdasarkan task type.

**4. Prompt Prefix & Slash Commands**

Implementasikan prefix yang sesuai dengan arsitektur Anda:

- **`@<path>`**: Attach file content ke prompt (diproses oleh MiniCPM Worker untuk summarization sebelum dikirim ke DeepSeek).
- **`!<command>`**: Jalankan shell command dan filter output-nya (gunakan output compressor yang sudah Anda rancang).
- **`/context`**: Trigger context compression manually.
- **`/escalate`**: Paksa eskalasi task ke DeepSeek dengan `reasoning_effort: "max"`.
- **`/stats`**: Tampilkan statistik token saving per session.
- **`/models`**: Browse dan switch model routing configuration.

**5. Diff View & Edit Review**

Implementasikan diff view yang memungkinkan user untuk mereview perubahan file yang dilakukan oleh agent. Anda bisa menggunakan library seperti `similar` atau `diffy` di Rust untuk menghasilkan diff, lalu menampilkannya di TUI dengan syntax highlighting.

**6. Modal Input: Normal & Insert Mode**

Implementasikan konsep modal input seperti OpenCode: user membaca di **Normal mode** (scrollback) dan menulis di **Insert mode**. Tekan **Esc** untuk masuk ke Normal mode, dan **i**/**a** untuk kembali ke Insert mode.

**7. Theme System**

Implementasikan theme system yang mendukung:

- **Built-in themes**: Minimal 5-10 theme (dark, light, high-contrast).
- **Custom themes**: Load dari file JSON di `~/.config/husk-cli/themes/`.
- **Terminal-adaptive**: Deteksi background color terminal dan pilih theme yang sesuai.

**8. Token Counter & Statistics**

Ini adalah fitur **pembeda** `husk-cli` dari OpenCode. Tampilkan di top bar atau status line:

- **Total tokens sent to DeepSeek**: Berapa token yang benar-benar dikirim ke model utama.
- **Total tokens processed by MiniCPM**: Berapa token yang diproses oleh model kecil.
- **Token saved**: Berapa token yang dihemat dibandingkan jika semua diproses DeepSeek.
- **Cost estimate**: Estimasi biaya per session.

**9. Client/Server Architecture**

Pisahkan TUI dari core logic. TUI adalah client yang berkomunikasi dengan server (core `husk-cli`) melalui local socket atau stdio. Ini memungkinkan:

- **Alternate frontends**: Web UI, desktop app, atau bahkan integrasi IDE.
- **Headless mode**: `husk run "task"` untuk scripting dan CI/CD.
- **Persistent sessions**: Session tetap berjalan meskipun TUI di-close.

---

### 📋 Ringkasan Implementasi UI/UX untuk `husk-cli`

| Komponen | OpenCode | `husk-cli` (Adaptasi) |
|---|---|---|
| **TUI Framework** | Ink/React | `ratatui` (Rust) |
| **Layout** | Top bar, Chat, Input | Top bar, Chat, Input + **Token Stats** |
| **Agent Switching** | Tab (Build ↔ Plan) | Tab (Supervisor ↔ Worker ↔ Auto) |
| **Prompt Prefix** | `@`, `!`, `use` | `@`, `!`, `/context`, `/escalate` |
| **Slash Commands** | `/new`, `/compact`, dll. | `/new`, `/context`, `/stats`, `/models` |
| **Diff View** | Built-in | Built-in dengan syntax highlighting |
| **Modal Input** | Normal/Insert (Vim-style) | Normal/Insert (Vim-style) |
| **Theme** | 50+ built-in, custom JSON | Built-in + custom JSON + terminal-adaptive |
| **Token Counter** | Basic | **Advanced** (DeepSeek vs MiniCPM savings) |
| **Arsitektur** | Client/Server | Client/Server + **Deterministic Routing Layer** |

Dengan mengimplementasikan komponen-komponen di atas, `husk-cli` akan memiliki UI/UX yang familiar bagi pengguna OpenCode, tetapi dengan **nilai tambah unik**: token counter yang menunjukkan penghematan nyata dari arsitektur hybrid DeepSeek + MiniCPM, serta kontrol manual untuk context compression dan escalation yang tidak dimiliki harness lain.

always CONSIDER UNTUK MENGGUNAKAN: <AGENTS>files you must read are inside @/spec/ and @/spec/handoff 
and you must write the spec and handoff after the entire task is completed, follow the previous way or existing way to write spec and handoff


so spec is the specs docs everything must be written here
and then handoff is after you finish one session you must write it here.

bacalah dari @/spec/handover/ folder itu isi semua handover document nya, pahami dan kemudian baca juga @/spec/ folder itu isi dari semua spec nya.
pahami semuanya


<REQUIREMENTS>
* NEVER ASSUME, ALWAYS ASK FOR ANY INCONSISTENCIES OR UNCLEAR
* NEVER USE EM DASH
</REQUIREMENTS>

anda adalah orang yang akan me research keseluruhan codebase ini untuk mendapatkan seluruh informasi yang pentin
tujuannya adalah untuk menjawab task ini :
<TASK>
{TASK}
</TASK>

pastikan anda mengerti dulu project ini itu ngapain dan juga isinya apa saja, struktur dan juga inti dari project ini cara kerja.
kemudian anda harus memahami dulu apa maksudnya dari task yang diberikan
setelah itu anda lakukan validasi search jadi anda search seluruh codebase untuk mengetahui dan memahami task dan berdasarkan codebase, dan juga seharusnya perubahan akan dilakukan dimana dan apa.
jadi research dan pahami dan deep thinking sampai anda mengerti semua tentang codebase ini kemudian juga apa hubungan dan maksud prompt/task nya.
setelah anda mengerti semua itu
tuliskan 
prompt nya apa
tujuan yang anda dapat/yang anda mengerti mau ngapain itu apa
dan juga kenapa mau tujuan itu  
terus codebasenya itu apa
dan juga perubahan yang mau di implement apa secara logic nya yang akan berubah apa.
terus perubahan secara code apa
kemudian perubahan code itu kenapa dilakukan
berikan juga logika nya atau pseudocode nya, untuk di review.
kemudian juga apabile user menyediakan testcase makan simulasikan dan juga berikan tracing nya supaya saat di review bisa. 
dan juga manual testing plan.

dan semua itu masukan kedalam sebuah markdown file.
didalam folder /spec/ isinya ikuti 3 digit mulai dari 000 sampai 999 jadi XXX. <task nya ngapain>.md
seperti itu, jangan lupa cek apakah sudah ada file didalam folder spec, kalau sudah ada coba cari siapa tau ada informasi yang related untuk mempercepat research.
pastikan isinya detail dan juga benar dan tervalidasi.

nanti kodingan di app/
pastikan untuk sebuah tasks supaya di design menggunakan multiple parallel subagents supaya lebih BERKUALITAS jadi supaya lebih berkualitas dan lebih sempurna dan juga supaya lebih cepat.
jadi harus lebih cepat pakai parallel subagents(minimal 5 parallel subagents).

</AGENTS> jadi gunakan agents.md yang saya tuliskan disini