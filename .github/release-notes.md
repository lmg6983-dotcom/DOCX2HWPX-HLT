<div align="center">

**워드 명세서(`.docx`)를 특허청 편집기용 파일로 바꾸는 도구**

`.docx` ➜ `.hlt` (NK-Editor) · `.hwpx` (MOIP-Editor)

</div>

## 🎯 이번 판 한눈에 — v0.1.5

> 표 · 그림이 많은 명세서가 HLT 변환 중 **문서 전체가 멈추던 문제**를 고쳤습니다.

### 🐛 고침

- **표가 많은 명세서** — HLT 변환 87%에서 `geometry_invalid (docx_table)`로 실패하던 문제
  - 칸이 모자라거나 남는 줄, 줄 앞뒤 빈 자리, 표 밖으로 넘친 칸 합치기, 위가 없는 세로 병합,
    소수 · 빠진 열 너비, 콘텐츠 컨트롤로 감싼 칸, 행이 없는 빈 표 등 **23가지 표 구조**를 HLT가 읽을 수 있게 고쳐 넣습니다.
  - 고친 표는 「표 n: …」 경고(`table_repaired`)로 알려 줍니다. 편집기에서 그 표를 확인해 주세요.
- **그림이 많은 명세서** — `structure_invalid (docx_relationships · docx_document)` · `size_limit_exceeded`로 실패하던 문제
  - SVG 그림은 PNG로 바꿔 넣습니다(PNG 대체 그림이 함께 있어도 실패했습니다).
  - 파일이 없거나, 깨졌거나, 읽을 수 없는 형식이거나, 「파일에 연결」로만 넣은 그림은 「그림 없음」 자리표를 넣고 `image_missing`으로 알려 줍니다.
  - HLT로 넘기는 문서가 64MB를 넘으면 그림 해상도를 200 → 150 → 100 → 72dpi로 낮춰 맞추고 `images_downscaled`로 알려 줍니다.

### 💬 다듬음

- 경고 · 오류 문구를 **어디서 — 무엇이 — 어떻게** 순서의 사용자 말로 바꿨습니다.
  - 어디서: `【0012】 부근 그림`, `【0015】 부근 표(문서의 3번째 표)`처럼 식별 번호로
  - 무엇이 · 어떻게: 무슨 일이 있었는지와 Word에서 할 일을 함께
  - 개발자에게 전할 내용은 맨 뒤 「기술 정보」에 두고, 화면 앱은 작게 보여 줍니다.

전체 기록은 [CHANGELOG.md](https://github.com/lmg6983-dotcom/DOCX_2_HWPX_OR_HLT/blob/main/CHANGELOG.md)에 있습니다.

## 📥 받기 — Windows 64비트

> 설치할 필요가 없습니다. 압축을 풀고 실행하면 됩니다.

| 파일 | 안에 든 것 | 이럴 때 |
|---|---|---|
| **`docx2hwpx-hlt-windows-x64.zip`** | 화면 앱 `docx2hwpx-hlt-gui.exe` | ⭐ 대부분 이것만 받으면 됩니다. DOCX를 끌어다 놓고 HLT/HWPX를 고른 뒤 「변환」 |
| `docx2hwpx-hlt-cli-windows-x64.zip` | 명령줄 `docx2hwpx-hlt.exe` | 일괄 변환 · 자동화 · PCT 서식(`--pct`) |

> [!TIP]
> 처음 실행할 때 Windows SmartScreen이 **「알 수 없는 게시자」** 라고 막으면 **추가 정보 → 실행**을 누릅니다.

| 출력 | 여는 곳 |
|---|---|
| `.hlt` | NK-Editor (통합명세서작성기) — 국내 출원 · PCT 국내진입 서식 |
| `.hwpx` | MOIP-Editor — 지식재산처가 고친 명세서 서식 |

> [!WARNING]
> HLT · HWPX는 공개 사양이 없는 형식입니다. 만든 파일은 **반드시 NK-Editor · MOIP-Editor에서 열어 확인한 뒤** 제출하세요.
