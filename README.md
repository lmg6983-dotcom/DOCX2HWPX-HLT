# DOCX_2_HWPX_OR_HLT

워드로 쓴 특허 명세서(`.docx`)를 특허청 전자출원 편집기용 파일로 바꿔 주는 무료 도구입니다.

| 만드는 파일 | 여는 프로그램 |
|---|---|
| `.hlt` | NK-Editor (통합명세서작성기) |
| `.hwpx` | MOIP-Editor |

> ⚠️ 두 형식 모두 공개 사양이 없습니다. 바꾼 파일은 **반드시 NK-Editor나 MOIP-Editor에서 열어 확인한 뒤** 제출하세요.

## 내려받기 (Windows 64비트)

설치할 필요가 없습니다. 압축을 풀고 실행하면 됩니다.

- **[화면 앱 받기](https://github.com/lmg6983-dotcom/DOCX_2_HWPX_OR_HLT/releases/latest/download/docx2hwpx-hlt-windows-x64.zip)**: 대부분 이것만 받으면 됩니다.
- [명령줄 버전 받기](https://github.com/lmg6983-dotcom/DOCX_2_HWPX_OR_HLT/releases/latest/download/docx2hwpx-hlt-cli-windows-x64.zip): 여러 파일을 한꺼번에 바꾸거나 자동화할 때, 또는 PCT 서식이 필요할 때 씁니다.

처음 실행할 때 Windows가 "알 수 없는 게시자"라며 막으면 **추가 정보 → 실행**을 누르세요.

## 쓰는 법

1. 워드 파일(`.docx`)을 창에 끌어다 놓습니다.
2. **HLT** 또는 **HWPX**를 고릅니다.
3. **변환**을 누릅니다.
4. 저장 창이 뜨면 저장합니다. 파일 이름은 그대로 두고 확장자만 바뀝니다(`명세서.docx` → `명세서.hlt`).

바꾸면서 손댄 글자나 확인할 곳이 있으면 창 아래에 목록으로 보여 줍니다.

## 자동으로 처리하는 것

- **수식**: 워드 수식(옛 수식 편집기 개체 포함)을 깔끔한 그림으로 넣습니다.
- **변경 내용 추적과 메모**: 추적한 변경은 모두 반영(수락)하고, 메모는 모두 지운 뒤 바꿉니다.
- **표와 그림**: 셀을 합친 표, WMF/EMF 그림도 옮깁니다.
- **HWPX 서식**: 지식재산처가 정한 명세서 서식(A4, 바탕체 12pt, 줄 간격 280% 등)에 맞춥니다.
- **HLT에서 쓸 수 없는 글자**: NK-Editor는 완성형 한글(CP949)에 있는 글자만 저장할 수 있어서 다음과 같이 바꿉니다.
  - `x²`, `H₂O`처럼 첨자 모양의 글자 → 보통 글자에 위/아래첨자 서식을 붙입니다.
  - `–`, `−`처럼 모양이 비슷한 글자 → `-`처럼 저장할 수 있는 글자로 바꿉니다.
  - `x̂`, `ü`처럼 바꿀 수 없는 글자 → 글자 모양 그대로 그림으로 넣습니다(편집기에서 글자로 고칠 수는 없습니다).

## 알아 둘 점

- 글상자 안의 글은 옮기지 않습니다.
- 워드 스타일에만 정해 둔 서식(글꼴, 크기 등)은 옮겨지지 않을 수 있습니다. 크기가 정해지지 않은 글은 12pt로 처리합니다.
- HLT는 표의 한 칸에 문단을 하나만 넣을 수 있습니다. 그래서 칸 안의 여러 문단은 한 문단으로 합치고, 칸 안에 든 표는 글로 풉니다.
- 화면 앱은 HLT를 국내 출원 서식으로만 만듭니다. PCT 국내진입 서식은 명령줄 버전에서 만들 수 있습니다.

---

## 명령줄 버전

```bash
docx2hwpx-hlt 명세서.docx --to hlt                  # → 명세서.hlt (국내 출원 서식)
docx2hwpx-hlt 명세서.docx --to hwpx                 # → 명세서.hwpx
docx2hwpx-hlt 명세서.docx --to hlt --pct -o 번역문.hlt   # PCT 국내진입 서식
```

- 같은 이름의 파일이 이미 있으면 멈춥니다. 덮어쓰려면 `--overwrite`를 붙이세요. 변환이 실패해도 원래 파일은 지워지지 않습니다.
- `--json`을 붙이면 결과를 JSON 한 줄로 출력합니다. 종료 코드는 0(성공), 1(변환 실패), 2(잘못된 인자)입니다. 자동화나 AI 에이전트로 쓸 때의 세부 규칙은 [AGENTS.md](AGENTS.md)에 있습니다.
- 수식 안의 한글과 글자 그림은 시스템 글꼴을 씁니다. 글꼴을 더 쓰려면 `--font-dir 폴더`로 지정하세요.

## 직접 빌드하기

[Rust](https://rustup.rs)가 필요합니다.

```bash
cargo build --release                                          # 명령줄 버전
cargo build --release --features gui --bin docx2hwpx-hlt-gui   # 화면 앱
cargo test                                                     # 시험
```

`v*` 태그를 올리거나 Actions의 `windows` 워크플로를 수동으로 실행하면서 `release`에 버전(예: `v0.1.2`)을 넣으면, 윈도 빌드와 릴리스가 자동으로 만들어집니다.

## 바탕이 된 오픈소스

| 이름 | 쓰는 곳 | 라이선스 |
|---|---|---|
| [seoan-editor](https://github.com/tornado1014/seoan-editor) `hlt-core` | DOCX 읽기, HLT 쓰기 | MIT |
| [HwpForge](https://github.com/ai-screams/HwpForge) | HWPX 쓰기 | MIT / Apache-2.0 |
| [Typst](https://github.com/typst/typst) | 수식, 글자 그림 | Apache-2.0 |
| [wmf-rs](https://github.com/mythrnr/wmf-rs), emf-core, resvg | WMF/EMF 그림 변환 | MIT, MIT / Apache-2.0 |

## 라이선스

[MIT](LICENSE). 릴리스용 exe 코드 서명 계획과 정책은 [CODE_SIGNING.md](CODE_SIGNING.md)에 있습니다.
