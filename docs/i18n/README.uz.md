<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="../assets/banner-dark.svg">
  <img alt="diskgc — dasturchi kompyuterlari uchun fonda ishlaydigan disk tozalagich" src="../assets/banner-light.svg" width="100%">
</picture>

**Nimani o‘chirayotganini biladigan, dasturchi kompyuterlari uchun fonda ishlaydigan disk tozalagich.**

[![CI](https://github.com/eukalpia/diskgc/actions/workflows/ci.yml/badge.svg)](https://github.com/eukalpia/diskgc/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/eukalpia/diskgc?display_name=tag&sort=semver)](https://github.com/eukalpia/diskgc/releases)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](../../LICENSE)
![Platforms](https://img.shields.io/badge/platforms-macOS%20%7C%20Linux%20%7C%20Windows-555)
![Rust 1.80+](https://img.shields.io/badge/rust-1.80%2B-orange.svg)

[English](../../README.md) · [Русский](README.ru.md) · **O‘zbekcha** · [简体中文](README.zh-CN.md) · [Español](README.es.md) · [हिन्दी](README.hi.md) · [العربية](README.ar.md) · [Français](README.fr.md) · [Português](README.pt-BR.md) · [日本語](README.ja.md)

<img alt="diskgc terminal boshqaruv paneli: umumiy ko‘rinish, har bir amal sababi bilan reja, hisobot va tarix" src="../assets/dashboard.gif" width="100%">

</div>

## Nega

Dasturchi disklari hech kim saqlashni so‘ramagan narsalar bilan to‘ladi: ikki kunda 100 GB ga o‘sadigan Cargo `target/` kataloglar, iyulda tashlab ketilgan branch’larning `node_modules` papkalari, har bir test ishga tushganda qayta to‘ladigan Go build keshlari, test to‘xtab qolganda qoldirilgan konteynerlar, AI kodlash agentlarining vaqtinchalik papkalari. macOS bularning hammasini "System Data" deb ataydi va yordam bermaydi; `du` esa *qayerda* ekanini ko‘rsatadi, *nimani xavfsiz o‘chirish mumkinligini* emas.

diskgc fonda kichik daemon sifatida ishlaydi, bo‘sh joy va uning o‘zgarish tendensiyasini kuzatib boradi hamda faqat vosita qayta tiklay oladigan narsalarni o‘chiradi — har bir amal uchun yozilgan sabab, hech narsa bo‘lishidan oldin o‘qib chiqish mumkin bo‘lgan reja va qancha joy bo‘shatilgani tarixi bilan.

## Asosiy xususiyatlar

- **O‘zini tushuntiradi.** Har bir amal sabab bilan keladi: *"target 105 GB (> 40 GB chegara): bir kundan eski hamma narsa"*, *"loyiha 30 kun davomida o‘zgarmagan; install bilan qayta tiklanadi"*.
- **Papkalarni emas, loyihalarni biladi.** `target/` mazmuniga qarab topiladi, build kataloglar esa faqat o‘z loyiha belgisi yonida bo‘ladi, faollik esa butun loyiha bo‘yicha baholanadi (git worktree alohida loyiha hisoblanadi).
- **Disk to‘lishidan oldin harakat qiladi.** So‘nggi olti soatlik prognoz tozalashni oldindan kuchaytiradi; chegaralar *normal* darajadan *low* va *critical* darajagacha qattiqlashadi.
- **Muhimiga hech qachon tegmaydi.** Manba kodlar, `.git`, ma’lumotlar bazalari va chat tarixlari hech qachon nomzod bo‘lmaydi; istisno qilingan glob’larga tegilmaydi; loglar o‘chirilmay, joyida siqiladi.
- **Sababini topadi.** `diskgc stats` kunlik va kategoriyalar bo‘yicha o‘sishni hamda qayta-qayta paydo bo‘ladigan yo‘llarni ko‘rsatadi — tozalash emas, tuzatish kerak bo‘lgan joylarni.
- **Terminal boshqaruv paneli va JSON.** Olti ekranli TUI va har bir buyruq uchun skriptlar uchun `--json`.

## Tezkor boshlash

```sh
cargo install --locked --git https://github.com/eukalpia/diskgc diskgc
diskgc config --edit      # [scan] roots ni loyihalaringiz joylashgan joyga o‘rnating
diskgc plan               # tozalash nimani va nega o‘chirishini ko‘rsatadi — hech narsaga tegilmaydi
diskgc install            # tizimga kirganda daemon’ni ishga tushiradi (launchd / systemd --user / Task Scheduler)
diskgc status
```

Tayyor binar fayllar macOS (Apple Silicon, Intel), Linux (x86_64) va Windows (x86_64) uchun har bir [reliz](https://github.com/eukalpia/diskgc/releases) ga biriktirilgan: `diskgc` (va ixtiyoriy ravishda `diskgc-ui`) ni arxivdan chiqarib, `PATH` dagi istalgan joyga qo‘ying.

<div align="center">
<img alt="terminalda diskgc plani: guruhlangan amallar, hajmlar va sabablar" src="../assets/plan.gif" width="100%">
</div>

## Nimani tozalaydi

| Qoida | Nima | Qachon |
|-------|------|--------|
| Rust target’lari | `target/` kataloglar (mazmuniga ko‘ra topiladi, qayerda bo‘lsa ham) | 7 kun davomida tegilmagan artefaktlar; 20 GB dan oshsa inkremental keshlar va katta test binarlari ham; 40 GB dan oshsa bir kundan eski hamma narsa; barcha target’lar uchun umumiy 40 GB byudjet |
| node_modules | `package.json` yonidagi `node_modules` | loyihada hech narsa 30 kun davomida o‘zgarmagan |
| Build kataloglar | `.dart_tool`, `build`, `_build`, `.next`, `.turbo`, `__pycache__`, `.gradle`, … loyiha belgisi yonida | o‘sha 30 kunlik qoida |
| Vaqtinchalik kataloglar | Claude Code’ning har bir sessiya uchun vaqtinchalik ish papkasi, tizim vaqtinchalik katalogidagi o‘z yozuvlaringiz, macOS’da foydalanuvchi `$TMPDIR` ichidagi eskirgan ish kataloglar | 3 kun / 7 kun / 24 soat davomida faol bo‘lmasa |
| AI vositalari | eski Codex ilova versiyalari, tashlab ketilgan runtime o‘rnatmalari | doim (joriy + 1 ta saqlanadi) |
| Docker | yetim testcontainers, to‘xtatilgan konteynerlar, egasiz volume va image’lar, build keshi | 2 soat / 24 soat / 7 kun |
| Vosita keshlari | chegaradan oshgan Go, npm, uv, pnpm va sccache keshlari, Chrome va VS Code yangilanish qoldiqlari, Homebrew yuklamalari, Xcode DerivedData, simulyator va Metal shader keshlari | qoidaning o‘z chegaralari bo‘yicha |
| Sovuq fayllar | Codex va Claude Code’ning sessiya loglari hamda JSONL tarixlari | fayl tizimi tomonidan **siqiladi** (APFS, NTFS), hech qachon o‘chirilmaydi |

## Qanday qaror qabul qiladi

Daemon har 10 daqiqada uyg‘onib, bo‘sh joyga qaraydi:

| Bo‘sh joy | Xatti-harakat |
|-----------|---------------|
| `comfortable_gb` (150) dan yuqori | har 6 soatda rejali tozalash |
| `comfortable_gb` dan past | odatdagi oynalar bilan hozir tozalash |
| `low_gb` (100) dan past | *low* daraja: barcha eskirish oynalari ikki baravar qisqaradi |
| `critical_gb` (60) dan past | *critical* daraja: oynalar to‘rt baravar qisqaradi, hajm chegaralari yarmiga tushadi |

So‘nggi olti soat snapshot’lari bo‘yicha eng kichik kvadratlar usuli (least-squares) bilan qurilgan chiziq 24 soatdan keyingi bo‘sh joyni bashorat qiladi; agar u chegaradan o‘tsa, daemon muddatidan oldin darajani oshiradi. Har bir ishga tushirish yozib boriladi, shuning uchun `diskgc stats` kunlik va kategoriyalar bo‘yicha o‘sishni hamda takroriy muammo manbalarini ko‘rsatadi.

## Xavfsizlik

diskgc fayllarni o‘chiradi — bu uning vazifasi — shuning uchun u oldindan aytib bo‘ladigan qilib qurilgan:

- U faqat vosita qayta tiklay oladigan narsalarni taklif qiladi (build chiqishlari, bog‘liqlik kataloglar, keshlar, vaqtinchalik fayllar) va har bir amal uchun sababni beradi; `plan` va `clean -n` hech narsaga tegmasdan aynan shu ro‘yxatni ko‘rsatadi.
- Manba kodlar, `.git`, ma’lumotlar bazalari va chat tarixlari hech qachon nomzod bo‘lmaydi; `[scan] exclude` ga mos keladigan narsaga hech qachon tegilmaydi; executor `[safety] allowed_prefixes` dan tashqaridagi har qanday yo‘lni rad etadi (standart holatda — uy katalogingiz hamda tizimning vaqtinchalik va kesh papkalari).
- `[safety] delete_mode = "trash"` narsalarni butunlay o‘chirish o‘rniga axlat qutisiga ko‘chiradi (macOS va Linux’da; axlat qutisi boshqa hajmda bo‘lsa, o‘chirishga o‘tadi).
- Har bir ishga tushirish logga yoziladi (`diskgc history`): nima rejalashtirilgani va haqiqatda nima bo‘shatilgani.

U hali ham sizning ruxsatlaringiz bilan nazoratsiz ishlaydi, shuning uchun daemon’ni o‘rnatishdan oldin kompyuteringizdagi birinchi `plan` natijasini diqqat bilan o‘qing.

## Terminal boshqaruv paneli

`diskgc ui` daemon bilan [IPC protokoli](../PROTOCOL.md) orqali bog‘lanadigan, [Cinder](https://github.com/eukalpia/cinder) asosida qurilgan boshqaruv panelini ochadi:

| Ekran | |
|-------|---|
| **Overview** | bo‘sh joy, daraja, prognoz, joy qayerga ketayotgani |
| **Plan** | har bir taklif qilingan amal va uning sababi — qolishi kerak bo‘lganlarning belgisini olib tashlang, qolganini bajaring yoki quruq ishga tushiring |
| **Report** | kategoriyalar bo‘yicha eng katta muammo manbalari |
| **History** | ishga tushirishlar, kunlik o‘sish, takroriy muammo manbalari |
| **Memory** | dasturlar bo‘yicha RAM, yetim yordamchi jarayonlar |
| **Log** | daemon’ning jonli faoliyati |

```
1-6 / tab   switch screens        r   refresh the current screen
p           build a plan          space / enter   toggle the highlighted action
x           execute selected      n   dry run             a   toggle all
k           terminate orphaned helper processes (Memory)   q   quit
```

Boshqaruv paneli alohida native executable (`diskgc-ui`) bo‘lib, `diskgc ui` uni `diskgc` yonida yoki `PATH` da topadi va agar daemon ishlamayotgan bo‘lsa, uni ishga tushiradi.

## Buyruqlar

```
diskgc report            diskda nima joy egallagani: kategoriyalar va eng katta muammo manbalari
diskgc plan              tozalash nima qilishi va nega — hech narsaga tegilmaydi
diskgc clean [-n]        hozir tozalash (-n = faqat reja)
diskgc status            daemon holati, bo‘sh joy, prognoz, oxirgi ishga tushirish
diskgc stats             kunlik o‘sish, kategoriya tendensiyalari, takroriy muammo manbalari
diskgc history           so‘nggi ishga tushirishlar va ular nimani o‘chirgani
diskgc mem               jarayonlar bo‘yicha RAM, yetim yordamchi jarayonlar (--kill-orphans)
diskgc install | uninstall
diskgc config [--edit|--show]
diskgc ui                terminal boshqaruv paneli
```

Har bir buyruq `--json` ni qabul qiladi.

## Sozlash

`~/.config/diskgc/config.toml` (Linux, macOS) yoki `%APPDATA%\diskgc\config.toml` (Windows) birinchi ishga tushirishda standart qiymatlar bilan yaratiladi va daemon har bir tsiklda qayta o‘qiydi. Har bir kalit [docs/CONFIG.md](../CONFIG.md) da tasvirlangan. Asosiylari:

```toml
[scan]
roots = ["~/work", "~/src"]          # loyihalaringiz joylashgan joylar
exclude = ["~/work/keep-forever/**"] # hech bir qoida tegishi mumkin bo‘lmagan glob’lar

[thresholds]
comfortable_gb = 150
low_gb = 100
critical_gb = 60

[safety]
delete_mode = "permanent"            # yoki "trash"
```

## Platformalar

| Platforma | Holati |
|-----------|--------|
| macOS (APFS) | kundalik ishlatiladi; asosiy maqsad |
| Linux | CI’da va konteynerlarda sinovdan o‘tgan; `systemd --user` bilan avtomatik ishga tushish |
| Windows | CI’da build qilinadi va testlardan o‘tadi; Task Scheduler orqali avtomatik ishga tushish eng kam sinovdan o‘tgan qism — fikr-mulohazalar kutiladi |

Shaffof siqish APFS va NTFS’da mavjud. Platformalar matritsasi uchun [docs/ARCHITECTURE.md](../ARCHITECTURE.md) ga qarang.

## Manba koddan yig‘ish

```sh
cargo build --release
cargo test
cd ui && dart pub get && dart compile exe bin/diskgc_ui.dart -o build/diskgc-ui
```

Ushbu README’dagi yozuvlarni sintetik uy katalogidan qayta yaratish mumkin: [docs/demo](../demo) bo‘limiga qarang.

## Hissa qo‘shish

Issue’lar va pull request’lar qabul qilinadi — [CONTRIBUTING.md](../../CONTRIBUTING.md) ga qarang. Yangi qoida uchun sintetik katalog daraxtida test kerak, u nimani o‘chirishini ham, nimaga tegmasligi kerakligini ham ko‘rsatishi shart.

## Litsenziya

[Apache-2.0](../../LICENSE).

*Bu — tarjima. Asosiy manba ingliz tilidagi [README](../../README.md) hisoblanadi.*
