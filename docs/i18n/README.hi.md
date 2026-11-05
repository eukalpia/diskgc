<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="../assets/banner-dark.svg">
  <img alt="diskgc — डेवलपर मशीनों के लिए एक बैकग्राउंड डिस्क क्लीनर" src="../assets/banner-light.svg" width="100%">
</picture>

**डेवलपर मशीनों के लिए एक बैकग्राउंड डिस्क क्लीनर, जो जानता है कि वह क्या मिटा रहा है।**

[![CI](https://github.com/eukalpia/diskgc/actions/workflows/ci.yml/badge.svg)](https://github.com/eukalpia/diskgc/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/eukalpia/diskgc?display_name=tag&sort=semver)](https://github.com/eukalpia/diskgc/releases)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](../../LICENSE)
![Platforms](https://img.shields.io/badge/platforms-macOS%20%7C%20Linux%20%7C%20Windows-555)
![Rust 1.80+](https://img.shields.io/badge/rust-1.80%2B-orange.svg)

[English](../../README.md) · [Русский](README.ru.md) · [O‘zbekcha](README.uz.md) · [简体中文](README.zh-CN.md) · [Español](README.es.md) · **हिन्दी** · [العربية](README.ar.md) · [Français](README.fr.md) · [Português](README.pt-BR.md) · [日本語](README.ja.md)

<img alt="diskgc का terminal dashboard: overview, हर action के कारण वाला plan, report और history" src="../assets/dashboard.gif" width="100%">

</div>

## क्यों

डेवलपर की डिस्क उन चीज़ों से भर जाती है जिन्हें किसी ने रखने को नहीं कहा: Cargo के `target/` directories जो दो दिनों में 100 GB बढ़ जाते हैं, जुलाई में छोड़ी गई branches के `node_modules`, हर test run से दोबारा भरे जाने वाले Go build caches, test के बीच में मरने के बाद पीछे छूटे containers, और AI coding agents की temp directories। macOS इन सबको "System Data" कहता है और कोई मदद नहीं करता; `du` बताता है *कहाँ*, पर यह नहीं बताता कि *क्या सुरक्षित रूप से मिटाया जा सकता है*।

diskgc एक छोटे daemon की तरह चलता है, free space और उसके trend पर नज़र रखता है, और सिर्फ़ वही हटाता है जिसे कोई tool दोबारा बना सके — हर action के साथ लिखा हुआ कारण, एक plan जिसे कुछ भी होने से पहले पढ़ा जा सके, और इसका इतिहास कि कितना जगह खाली हुई।

## मुख्य विशेषताएँ

- **अपना कारण खुद बताता है।** हर action के साथ एक कारण आता है: *"target is 105 GB (> 40 GB cap): everything older than a day"*, *"project untouched for 30 days; restored by install"*।
- **फ़ोल्डर नहीं, projects समझता है।** `target/` उसके content से पहचाना जाता है, build directories सिर्फ़ अपने project marker के पास मिलती हैं, और activity पूरे project पर आँकी जाती है (git worktree अपना अलग project माना जाता है)।
- **डिस्क भरने से पहले कार्रवाई करता है।** पिछले छह घंटों के snapshots पर आधारित forecast के हिसाब से सफ़ाई पहले ही तेज़ हो जाती है; thresholds *normal* से *low* और फिर *critical* तक कसते जाते हैं।
- **जो ज़रूरी है उसे नहीं छूता।** Source, `.git`, databases और chat histories कभी candidate नहीं बनते; excluded globs कभी नहीं छुए जाते; logs को हटाया नहीं जाता, बल्कि filesystem के ज़रिए compress किया जाता है।
- **कारण ढूँढता है।** `diskgc stats` दिखाता है कि हर दिन और हर category में कितना बढ़ रहा है, और कौन-से paths बार-बार लौट आते हैं — यानी वे जगहें जिन्हें साफ़ करने के बजाय ठीक करना चाहिए।
- **Terminal dashboard और JSON।** छह screens वाला TUI, और scripts के लिए हर command पर `--json`।

## त्वरित शुरुआत

```sh
cargo install --locked --git https://github.com/eukalpia/diskgc diskgc
diskgc config --edit      # [scan] roots में उन जगहों को रखें जहाँ आपके projects हैं
diskgc plan               # clean क्या हटाएगा और क्यों — कुछ भी नहीं छुआ जाता
diskgc install            # login पर daemon शुरू करें (launchd / systemd --user / Task Scheduler)
diskgc status
```

macOS (Apple Silicon, Intel), Linux (x86_64) और Windows (x86_64) के लिए तैयार binaries हर [release](https://github.com/eukalpia/diskgc/releases) के साथ संलग्न हैं: `diskgc` (और चाहें तो `diskgc-ui`) को अपने `PATH` पर किसी भी जगह extract कर दें।

<div align="center">
<img alt="terminal में diskgc plan: आकार और कारणों के साथ समूहबद्ध actions" src="../assets/plan.gif" width="100%">
</div>

## क्या साफ़ होता है

| नियम | क्या | कब |
|------|------|------|
| Rust targets | `target/` directories (content से पहचानी जाती हैं, चाहे वे कहीं भी हों) | artifacts 7 दिन से नहीं छुए गए; 20 GB से ऊपर incremental caches और भारी test binaries भी; 40 GB से ऊपर एक दिन से पुरानी हर चीज़; सभी targets के लिए कुल 40 GB का budget |
| node_modules | `package.json` के बगल में `node_modules` | project में 30 दिन से कुछ नहीं बदला |
| Build directories | `.dart_tool`, `build`, `_build`, `.next`, `.turbo`, `__pycache__`, `.gradle`, … किसी project marker के बगल में | वही 30-दिन वाला नियम |
| Temporary directories | Claude Code का per-session scratch, system temp directory में आपकी अपनी entries, per-user `$TMPDIR` (macOS) में पुराने work directories | 3 दिन / 7 दिन / 24 घंटे तक शांत |
| AI tools | पुराने Codex application versions, छोड़े गए runtime installs | हमेशा (current + 1 रखे जाते हैं) |
| Docker | अनाथ testcontainers, रुके हुए containers, dangling volumes और images, build cache | 2 घंटे / 24 घंटे / 7 दिन |
| Tool caches | सीमा से ऊपर Go, npm, uv, pnpm और sccache caches, Chrome और VS Code के update के बचे हुए हिस्से, Homebrew downloads, Xcode DerivedData, simulator और Metal shader caches | हर नियम की अपनी सीमा |
| Cold files | Codex और Claude Code के session logs और JSONL histories | filesystem (APFS, NTFS) द्वारा **compress** किए जाते हैं, कभी delete नहीं |

## यह कैसे तय करता है

Daemon हर 10 मिनट पर जागकर free space देखता है:

| Free space | व्यवहार |
|------------|-----------|
| `comfortable_gb` (150) से ऊपर | हर 6 घंटे पर scheduled hygiene |
| `comfortable_gb` से नीचे | सामान्य windows के साथ अभी clean |
| `low_gb` (100) से नीचे | *low* level: हर staleness window आधी |
| `critical_gb` (60) से नीचे | *critical* level: windows एक-चौथाई, size caps आधे |

पिछले छह घंटों के snapshots पर least-squares fit 24 घंटे बाद के free space का अनुमान लगाता है; अगर वह कोई threshold पार करता है, तो daemon पहले से ही level बढ़ा देता है। हर run record होता है, इसलिए `diskgc stats` दिखाता है कि हर दिन और हर category में कितनी growth है, और बार-बार आने वाले offenders कौन हैं।

## सुरक्षा

diskgc files मिटाता है — यही उसका काम है — इसलिए उसे predictable बनाकर रखा गया है:

- यह सिर्फ़ वही प्रस्तावित करता है जिसे कोई tool दोबारा बना सके (build output, dependency directories, caches, temp files), और हर action का कारण देता है; `plan` और `clean -n` बिना कुछ छुए ठीक वही सूची दिखाते हैं।
- Source, `.git`, databases और chat histories कभी candidate नहीं बनते; `[scan] exclude` से मेल खाने वाली किसी भी चीज़ को नहीं छुआ जाता; executor `[safety] allowed_prefixes` के बाहर का कोई भी path मना कर देता है (डिफ़ॉल्ट रूप से आपकी home directory और system के temp व cache folders)।
- `[safety] delete_mode = "trash"` चीज़ों को स्थायी रूप से मिटाने के बजाय trash में भेजता है (macOS और Linux पर; अगर trash किसी दूसरे volume पर है तो यह delete पर लौट आता है)।
- हर run `diskgc history` में log होता है, जिसमें लिखा रहता है कि क्या plan हुआ था और असल में कितना जगह खाली हुई।

यह फिर भी आपकी permissions के साथ बिना निगरानी के चलता है, इसलिए daemon install करने से पहले अपनी मशीन पर पहला `plan` ज़रूर पढ़ें।

## Terminal dashboard

`diskgc ui` एक dashboard खोलता है, जो [Cinder](https://github.com/eukalpia/cinder) से बना है और अपने [IPC protocol](../PROTOCOL.md) के ज़रिए daemon से बात करता है:

| Screen | |
|--------|---|
| **Overview** | free space, level, forecast, जगह कहाँ जा रही है |
| **Plan** | हर प्रस्तावित action और उसका कारण — जो रखना हो उसका tick हटा दें, बाकी execute करें, या dry-run करें |
| **Report** | हर category के सबसे बड़े offenders |
| **History** | runs, हर दिन की growth, बार-बार आने वाले offenders |
| **Memory** | program के हिसाब से RAM, अनाथ helper processes |
| **Log** | daemon की live activity |

```
1-6 / tab   switch screens        r   refresh the current screen
p           build a plan          space / enter   toggle the highlighted action
x           execute selected      n   dry run             a   toggle all
k           terminate orphaned helper processes (Memory)   q   quit
```

Dashboard एक अलग native executable (`diskgc-ui`) है; `diskgc ui` उसे `diskgc` के बगल में या `PATH` पर ढूँढता है, और अगर कोई daemon नहीं चल रहा हो तो उसे शुरू कर देता है।

## Commands

```
diskgc report            disk पर क्या है: categories और सबसे बड़े offenders
diskgc plan              clean क्या करेगा, कारणों के साथ — कुछ भी नहीं छुआ जाता
diskgc clean [-n]        अभी clean करें (-n = सिर्फ़ plan)
diskgc status            daemon की स्थिति, free space, forecast, आखिरी run
diskgc stats             हर दिन की growth, category trends, बार-बार आने वाले offenders
diskgc history           हाल के runs और उन्होंने क्या हटाया
diskgc mem               process के हिसाब से RAM, अनाथ helper processes (--kill-orphans)
diskgc install | uninstall
diskgc config [--edit|--show]
diskgc ui                terminal dashboard
```

हर command `--json` स्वीकार करती है।

## कॉन्फ़िगरेशन

`~/.config/diskgc/config.toml` (Linux, macOS) या `%APPDATA%\diskgc\config.toml` (Windows) पहली बार चलने पर default settings के साथ बनता है, और daemon हर tick पर उसे दोबारा पढ़ता है। हर key का विवरण [docs/CONFIG.md](../CONFIG.md) में है। ज़रूरी बातें:

```toml
[scan]
roots = ["~/work", "~/src"]          # जहाँ आपके projects रहते हैं
exclude = ["~/work/keep-forever/**"] # ऐसे globs जिन्हें कोई नियम न छुए

[thresholds]
comfortable_gb = 150
low_gb = 100
critical_gb = 60

[safety]
delete_mode = "permanent"            # या "trash"
```

## प्लेटफ़ॉर्म

| प्लेटफ़ॉर्म | स्थिति |
|----------|--------|
| macOS (APFS) | रोज़ इस्तेमाल होता है; मुख्य target |
| Linux | CI और containers में tested; `systemd --user` से autostart |
| Windows | CI में build होता है और tests पास करता है; Task Scheduler से autostart सबसे कम परखा गया हिस्सा है — रिपोर्ट का स्वागत है |

Transparent compression APFS और NTFS पर उपलब्ध है। Platform matrix के लिए [docs/ARCHITECTURE.md](../ARCHITECTURE.md) देखें।

## Source से build करना

```sh
cargo build --release
cargo test
cd ui && dart pub get && dart compile exe bin/diskgc_ui.dart -o build/diskgc-ui
```

इस README की recordings एक synthetic home directory से दोबारा बनाई जा सकती हैं: देखें [docs/demo](../demo)।

## योगदान

Issues और pull requests का स्वागत है — देखें [CONTRIBUTING.md](../../CONTRIBUTING.md)। नए नियम के लिए एक synthetic directory tree पर test चाहिए, जो दिखाए कि नियम क्या हटाता है और क्या अछूता छोड़ना ज़रूरी है।

## लाइसेंस

[Apache-2.0](../../LICENSE)।

---

*यह README का अनुवाद है। मूल अंग्रेज़ी संस्करण ही संदर्भ है: [README.md](../../README.md)।*
