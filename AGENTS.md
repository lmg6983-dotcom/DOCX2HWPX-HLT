# 에이전트 안내 — docx2hwpx-hlt

워드 명세서(`.docx`) → `.hwpx`(MOIP-Editor) · `.hlt`(NK-Editor). 사람 설명은 [README.md](README.md).

## 쓰기

```bash
cargo build --release   # 처음 한 번. 바이너리: target/release/docx2hwpx-hlt
target/release/docx2hwpx-hlt 명세서.docx --to hlt --json
```

- 늘 `--json` 을 준다. 표준 출력에 JSON **한 줄**만 나온다(성공 · 실패 모두). 사람용 글은 내지 않는다.
- 대화형 입력이 없다. 네트워크도 쓰지 않는다.
- 출력이 이미 있으면 멈춘다(`output_exists`). 다시 만들 때는 지우지 말고 `--overwrite` — 끝까지 쓴 뒤 바꿔 넣으므로 실패해도 원래 파일이 남는다.
- `--pct` 는 HLT 의 PCT 국내진입 서식. 사용자가 PCT 번역문이라고 할 때만 준다.

### 종료 코드

| 코드 | 뜻 |
|---|---|
| 0 | 성공(경고가 있어도 0) |
| 1 | 변환 실패 — `error.code` 를 본다 |
| 2 | 인자 잘못(`error.code` = `usage`) |

### JSON

```json
{"ok":true,"input":"a.docx","output":"a.hlt","format":"hlt","dialect":"domestic",
 "revisions_accepted":7,"comments_removed":1,"equations":6,"metafiles":2,"scripts":4,
 "substitutions":[{"from":"–","codepoints":"U+2013","to":"-","count":1}],
 "glyph_images":[{"text":"x̂","codepoints":"U+0078 U+0302","count":1}],
 "warnings":[{"code":"nested_table_flattened","message":"…"}]}
```

| 필드 | 뜻 |
|---|---|
| `dialect` | HLT: `domestic` · `pct`, HWPX: `null` |
| `revisions_accepted` | 받아들인 변경 추적 수(넣기 · 지우기 · 옮기기 · 서식 변경 · 지운 문단 표시 · 줄/칸) — 변환은 늘 모두 받아들인 상태로 한다 |
| `comments_removed` | 지운 메모 수 — 메모는 늘 모두 지운다(본문 표시 · comments*.xml) |
| `equations` | 그림으로 넣은 수식(OMML) 수 |
| `metafiles` | PNG 로 바꾼 WMF/EMF(OLE 수식 미리보기 포함) 수 |
| `scripts` | 첨자 모양 글자(², ₂)를 첨자 서식으로 바꾼 곳 수 (HLT) |
| `substitutions` | 모양이 같은 글자로 바꾼 것 (HLT) |
| `glyph_images` | 글자 그림으로 넣은 글자 (HLT) — 편집기에서 글로 고칠 수 없다 |
| `warnings` | 아래 표 |
| `error` | 실패일 때만 `{code, message}` |

입력 · 출력 경로 실패 뒤에는 수치 필드가 없을 수 있다 — `ok` 를 먼저 본다.

### 실패 코드 (`error.code`)

| 코드 | 할 일 |
|---|---|
| `usage` | 인자를 고친다(`--help`) |
| `input_unreadable` | 경로를 확인한다 |
| `invalid_docx` | `.docx` 가 아니거나 깨졌다(`.doc` 이면 사용자에게 Word 에서 `.docx` 로 저장해 달라고 한다) |
| `output_exists` | `--overwrite` 또는 다른 `-o` |
| `convert_failed` | HLT/HWPX 쓰기 실패. `message` 를 사용자에게 그대로 전한다 — 문서 구조 문제가 대부분이다 |
| `write_failed` | 출력 폴더 · 권한 · 디스크 |

### 경고 코드 (`warnings[].code`) — 성공이지만 사람이 확인할 곳

| 코드 | 뜻 |
|---|---|
| `equation_as_text` | 수식을 그리지 못해 글로 남겼다 |
| `metafile_placeholder` | WMF/EMF 를 그리지 못해 「그림 변환 실패」 그림을 넣었다 |
| `metafile_failed` | 자리표 그림도 넣지 못했다 |
| `symbol_unmapped` | Symbol 밖 기호 글꼴(Wingdings 등) 글자를 옮기지 못했다 |
| `glyph_failed` | 글자 그림을 그리지 못해 원래 글자를 남겼다(HLT 에서 실패로 이어질 수 있다) |
| `nested_table_flattened` | HLT: 표 칸 안의 표를 글로 풀었다 |
| `image_missing` | HWPX: DOCX 안에 그림 파일이 없다 |

### 사용자에게 알릴 것

변환 뒤 `warnings` · `glyph_images` · `substitutions` 가 비어 있지 않으면 그 목록을 짧게 전하고,
**NK-Editor / MOIP-Editor 에서 열어 확인한 뒤 제출하라**고 덧붙인다. 공개 사양이 없는 형식이다.

## 고치기 (이 저장소에서 일할 때)

| 파일 | 내용 |
|---|---|
| `src/lib.rs` | 변환 핵심 `convert`(바이트 → 바이트 + 보고 · 진행 알림) · `write_atomic` |
| `src/main.rs` | 명령줄: 인자 · JSON · 종료 코드 |
| `src/bin/gui.rs` | 화면 앱(egui · `gui` 기능) |
| `src/normalize.rs` | DOCX 고쳐 쓰기(수식 → 그림, WMF/EMF, 변경 추적, 칸 합치기, HLT 글자 규칙 적용) |
| `src/chars.rs` | NK-Editor 글자 규칙(CP949) |
| `src/omml.rs` | OMML → Typst 수식 |
| `src/render.rs` | Typst · resvg 로 PNG 굽기 |
| `src/hwpx.rs` | 고쳐 쓴 DOCX → HWPX(HwpForge) |
| `tests/make_fixture.py` | 시험용 DOCX 생성기 → `tests/fixtures/sample.docx` |

```bash
cargo fmt && cargo clippy --all-targets && cargo test
cargo clippy --features gui --bin docx2hwpx-hlt-gui   # 화면 앱을 고쳤을 때
python3 tests/make_fixture.py   # 고정 자료를 바꿨을 때만
```

- JSON 필드 · 실패 코드 · 경고 코드는 계약이다. 바꾸면 이 파일과 `tests/convert.rs` 를 함께 고친다.
- 새 경고는 `normalize::Warnings` 에 코드와 함께 넣고 위 표에 더한다.
- 실제 출원 명세서는 저장소에 넣지 않는다.
