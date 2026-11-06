# 🤝 기여 안내

DOCX_2_HWPX_OR_HLT에 관심을 가져 주셔서 고맙습니다.
버그 제보 · 제안 · 코드 기여 모두 환영합니다.

- [🐛 버그 제보](#-버그-제보)
- [🛠️ 개발 환경](#%EF%B8%8F-개발-환경)
- [✍️ 커밋 메시지](#%EF%B8%8F-커밋-메시지)
- [🔀 풀 리퀘스트](#-풀-리퀘스트)
- [🚀 릴리스](#-릴리스)

> [!CAUTION]
> **실제 출원 명세서는 이슈 · PR · 저장소 어디에도 올리지 마세요.**
> 문제를 재현하는 최소 예시 문서를 만들거나, 내용을 지운 문서를 써 주세요.

## 🐛 버그 제보

[이슈 양식](https://github.com/lmg6983-dotcom/DOCX_2_HWPX_OR_HLT/issues/new/choose)을 써 주세요. 다음이 있으면 훨씬 빨리 고칠 수 있습니다.

1. **판 번호** — 화면 앱 바닥글, 또는 `docx2hwpx-hlt --version`
2. **형식** — HLT(국내 · PCT) / HWPX
3. **오류 · 경고 문구 전체** — 특히 맨 뒤 「기술 정보」
4. 명령줄이라면 `--json` 출력 한 줄

## 🛠️ 개발 환경

[Rust](https://rustup.rs) stable(2024 edition)이 필요합니다.

```bash
git clone https://github.com/lmg6983-dotcom/DOCX_2_HWPX_OR_HLT.git
cd DOCX_2_HWPX_OR_HLT

cargo build --release                               # 명령줄
cargo build --release --features gui --bins         # 명령줄 + 화면 앱
```

PR을 열기 전에 다음이 모두 통과해야 합니다.

```bash
cargo fmt && cargo clippy --all-targets && cargo test
cargo clippy --features gui --bin docx2hwpx-hlt-gui   # 화면 앱을 고쳤을 때
python3 tests/make_fixture.py                         # 고정 자료를 바꿨을 때만
```

> [!IMPORTANT]
> `--json` 필드 · 실패 코드 · 경고 코드는 **계약**입니다. 바꾸면 [AGENTS.md](AGENTS.md)와 `tests/convert.rs`를 함께 고칩니다.
> 새 경고는 `normalize::Warnings`에 코드와 함께 넣고 AGENTS.md 표에 더합니다.

## ✍️ 커밋 메시지

[Conventional Commits](https://www.conventionalcommits.org/ko/v1.0.0/)를 따릅니다. 제목은 한국어로 써도 됩니다.

```text
<종류>(<범위>): <무엇을 했는지 — 50자 안팎, 마침표 없이>

<왜 바꿨는지 · 무엇이 문제였는지 — 72자에서 줄 바꿈>

- 바꾼 것 1
- 바꾼 것 2

Fixes #12
```

| 종류 | 언제 | 릴리스 판 |
|---|---|---|
| `feat` | ✨ 새 기능 | MINOR (`0.x` 동안은 PATCH) |
| `fix` | 🐛 버그 고침 | PATCH |
| `docs` | 📝 문서만 | — |
| `refactor` | ♻️ 동작이 같은 코드 정리 | — |
| `perf` | ⚡ 빠르게 · 작게 | PATCH |
| `test` | ✅ 시험 더하기 · 고치기 | — |
| `build` · `ci` | 👷 Cargo · GitHub Actions | — |
| `chore` | 🔧 그 밖의 잡일 | — |

**범위**는 `hlt` · `hwpx` · `gui` · `cli` · `omml` · `tables` · `shapes` · `chars` 처럼 손댄 곳을 씁니다.
JSON 계약을 깨는 변경은 `feat(cli)!:`처럼 `!`를 붙이고 본문에 `BREAKING CHANGE:`를 적습니다.

<details>
<summary><b>예시</b></summary>

```text
fix(hlt): 위가 없는 세로 병합 칸을 새 병합 시작으로 바꾼다

hlt-core 의 표 판독은 표 하나라도 어긋나면 문서 전체를 멈춘다.
vMerge 이어짐 칸 위에 같은 폭의 병합 시작이 없으면 geometry_invalid 로 실패했다.

- tables.rs: 위가 없는 vMerge 이어짐 → restart
- 고정 자료에 그런 표 하나, 단위 시험 하나

Fixes #13
```

```text
feat(gui): 변환 뒤 「폴더 열기」 단추
```

```text
docs: README 내려받기 단추 · 배지 정리
```

</details>

이 저장소에 든 커밋 템플릿을 쓰면 편합니다.

```bash
git config commit.template .gitmessage
```

## 🔀 풀 리퀘스트

- `main`에서 가지를 쳐서 작업합니다(`fix/hlt-vmerge`, `feat/gui-open-folder` 등).
- PR 제목도 커밋 규칙을 따릅니다 — squash 병합하면 PR 제목이 커밋 제목이 됩니다.
- PR에 라벨(`feature` · `bug` · `documentation` · `ci`)을 달면 릴리스 노트에서 알맞은 분류로 묶입니다([`.github/release.yml`](.github/release.yml)).
- 사용자에게 보이는 변경이면 [CHANGELOG.md](CHANGELOG.md)의 `[Unreleased]`에 한 줄 더합니다.

## 🚀 릴리스

1. `Cargo.toml`의 `version`을 올립니다(`cargo build`로 `Cargo.lock`도 함께 갱신).
2. [CHANGELOG.md](CHANGELOG.md)의 `[Unreleased]`를 새 판 번호와 날짜로 바꾸고, 맨 아래 비교 링크를 더합니다.
3. [.github/release-notes.md](.github/release-notes.md)의 「이번 판 한눈에」를 새 판 내용으로 바꿉니다.
4. 커밋 `chore(release): v0.1.6`을 `main`에 합칩니다.
5. 태그를 올리거나(`git tag v0.1.6 && git push origin v0.1.6`), Actions → `windows` → **Run workflow**에서 `release`에 `v0.1.6`을 넣습니다.

워크플로가 윈도에서 시험 · 빌드한 뒤 두 zip을 붙인 릴리스를 만들고, `release-notes.md` 뒤에 PR 목록을 자동으로 붙입니다.
코드 서명 설정은 [CODE_SIGNING.md](CODE_SIGNING.md)를 보세요.

## 📄 라이선스

기여한 코드는 이 프로젝트와 같은 [MIT 라이선스](LICENSE)로 배포됩니다.
