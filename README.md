# DOCX2HWPX-HLT

워드 명세서(`.docx`)를 특허청 편집기용 파일로 바꾸는 명령줄 도구 (Rust).

| 출력 | 읽는 곳 |
|---|---|
| `.hwpx` | MOIP-Editor |
| `.hlt` | NK-Editor (통합명세서작성기) |

```bash
cargo build --release
./target/release/docx2hwpx-hlt 명세서.docx --to hwpx            # → 명세서.hwpx
./target/release/docx2hwpx-hlt 명세서.docx --to hlt             # → 명세서.hlt (국내 출원 서식)
./target/release/docx2hwpx-hlt 명세서.docx --to hlt --pct -o 번역문.hlt   # PCT 국내진입 서식
```

출력 파일이 이미 있으면 멈춘다(`--overwrite` 로 바꿔 쓰기 — 끝까지 쓴 뒤 바꿔 넣으므로 실패해도 원래 파일은 남는다).
바꾼 글자 · 그림으로 넣은 글자 · 경고는 표준 오류에 찍는다. `--json` 을 주면 같은 내용을 JSON 한 줄로 표준 출력에 낸다.
종료 코드는 0 성공(경고가 있어도) · 1 변환 실패 · 2 인자 잘못.

에이전트(Claude Code 등)로 쓸 때의 계약(JSON 필드 · 실패/경고 코드)은 [AGENTS.md](AGENTS.md).

## 깨지기 쉬운 것을 어떻게 다루나

변환 전에 DOCX 를 한 번 고쳐 쓴다(`src/normalize.rs`). 두 출력 모두 이 단계를 거친다.

| 대상 | 처리 |
|---|---|
| 수식(OMML) | Typst 로 300dpi PNG 를 그려 그 자리에 그림으로 넣는다. 별행 수식 문단은 가운데 정렬. 그리지 못하면 글로라도 남기고 경고 |
| 옛 OLE 수식(Equation 3.0 · MathType) | WMF 미리보기를 PNG 로 바꾸고, VML 크기 그대로 넣는다 |
| WMF · EMF 그림 | SVG 를 거쳐 PNG 로. 실패하면 「그림 변환 실패」 자리표 그림 + 경고 |
| 표 | 가로 합침(gridSpan) · 세로 합침(vMerge) · gridBefore/After. HWPX 는 칸 안 여러 문단과 표 안 표까지 그대로 옮긴다 |
| 변경 추적 | 넣은 글은 받아들이고, 지운 글은 버린다 |
| `w:sym`(Symbol 글꼴) | 유니코드 글자로(β, ±, → …) |

### NK-Editor(HLT) 글자 규칙

HLT 는 글을 CP949 로 저장하므로 그 밖의 글자는 살아남지 못한다. 그래서 HLT 로 낼 때만 다음을 더 한다(`src/chars.rs`).

1. 첨자 모양 글자(`²` `⁻³` `₂` `ₙ` …) → 보통 글자 + 위/아래첨자 **서식**
2. 모양이 같은 CP949 글자로 바꾸기: `–` `‐` `−` → `-`, `—` → `―`, 특수 공백 → 공백, 합자(`ﬁ`) → `fi` 등
3. 그 밖(`x̂`·`ŷ` 같은 hat/결합 문자, 악센트 글자 `ü`, 낯선 기호) → **글자 그림**. 악센트를 떼는 바꾸기(é → e)는 뜻이 바뀌므로 하지 않는다

HLT 표 칸은 문단을 하나만 가질 수 있어, 칸 안 여러 문단은 띄어 쓴 한 문단으로, 칸 안 표는 `칸 | 칸 / 다음 줄` 글로 푼다(경고가 찍힌다).

## 바탕이 된 오픈소스

| 이름 | 쓰는 곳 | 라이선스 |
|---|---|---|
| [seoan-editor](https://github.com/tornado1014/seoan-editor) `hlt-core` | DOCX 판독, HLT 굽기 · 검증 | MIT |
| [HwpForge](https://github.com/ai-screams/HwpForge) | HWPX 쓰기 | MIT / Apache-2.0 |
| [Typst](https://github.com/typst/typst) | 수식 · 글자 그림 | Apache-2.0 |
| [wmf-rs](https://github.com/mythrnr/wmf-rs) · emf-core · resvg | WMF/EMF → PNG | MIT · MIT/Apache-2.0 |

## 글꼴

수식은 Typst 내장 글꼴(New Computer Modern Math)로 그린다. 수식 안 한글 · 글자 그림은 시스템 글꼴(바탕, 맑은 고딕, 나눔 …)을 찾아 쓰고, 더 필요하면 `--font-dir 폴더` 로 더한다.

## 시험

```bash
python3 tests/make_fixture.py   # tests/fixtures/sample.docx 다시 만들기(수식 · 합친 표 · 표 안 표 · WMF/EMF · 특수 글자)
cargo test                      # 두 형식으로 바꾼 뒤 다시 읽어 확인
```

## 주의

- HLT · HWPX 모두 공개 사양이 없거나 일부만 있어, 만든 파일은 **반드시 NK-Editor · MOIP-Editor 에서 열어 확인한 뒤** 제출한다.
- 글자 크기를 적지 않은 글은 12pt 로 본다. 문단 · 글자 스타일(styles.xml)의 상속은 옮기지 않는다(런에 직접 단 서식만).
- 글상자(text box) 안의 글은 옮기지 않는다.
