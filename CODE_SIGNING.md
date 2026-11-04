# 코드 서명

윈도에서 exe를 처음 열 때 뜨는 "알 수 없는 게시자" 경고를 없애기 위해,
릴리스용 exe에 [SignPath Foundation](https://signpath.org)의 무료 오픈소스 코드 서명을 적용합니다.

> 지금은 **신청 전** 상태입니다. 아래 설정을 마치기 전까지 릴리스는 서명 없이 나갑니다.
> 서명을 해도 경고가 바로 사라지지는 않습니다. SmartScreen은 내려받은 사람이 늘수록 평판을 쌓고, 평판이 쌓여야 경고를 멈춥니다.

## 설정 순서 (저장소 관리자)

1. **신청**: https://signpath.org/apply 에서 이 저장소로 신청합니다.
   - 라이선스는 MIT입니다([LICENSE](LICENSE)).
   - 신청서에 서명 정책 URL을 적는 칸이 있으면 이 파일의 주소를 적습니다.
     `https://github.com/lmg6983-dotcom/DOCX2HWPX-HLT/blob/main/CODE_SIGNING.md`
   - GitHub와 SignPath 계정 모두 2단계 인증(MFA)을 켜 둡니다.
2. **SignPath 설정** (승인 뒤 SignPath 웹에서)
   - GitHub 연동(Trusted Build System: GitHub.com)을 추가하고 이 저장소에 연결합니다.
   - Artifact Configuration에 [`.signpath/artifact-configuration.xml`](.signpath/artifact-configuration.xml) 내용을 붙여 넣습니다.
   - 서명 정책(signing policy) 이름(slug)을 확인합니다. 보통 `release-signing`입니다.
   - 서명 요청을 보낼 CI 사용자(CI user)를 만들고 API 토큰을 발급합니다.
3. **GitHub 저장소 설정** (Settings → Secrets and variables → Actions)

   | 종류 | 이름 | 값 |
   |---|---|---|
   | Secret | `SIGNPATH_API_TOKEN` | 2단계에서 발급한 API 토큰 |
   | Variable | `SIGNPATH_ORGANIZATION_ID` | SignPath 조직 ID |
   | Variable | `SIGNPATH_PROJECT_SLUG` | 프로젝트 slug. 비우면 `DOCX2HWPX-HLT` |
   | Variable | `SIGNPATH_SIGNING_POLICY_SLUG` | 서명 정책 slug. 비우면 `release-signing` |
   | Variable | `SIGNPATH_ARTIFACT_CONFIGURATION_SLUG` | (선택) 비우면 기본 설정 |

4. **릴리스**: Actions → `windows` → **Run workflow**를 누르고 `release`에 `v0.1.2`처럼 버전을 넣습니다.
   - 서명 요청이 가면 SignPath에서 승인(approve)을 눌러야 진행됩니다. 1시간 안에 승인하지 않으면 실패합니다.
   - 서명이 끝나면 워크플로가 두 exe의 서명이 올바른지 확인한 뒤 zip을 만들어 릴리스합니다.
   - 서명 정책이 `main` 브랜치의 빌드만 허용하면 **`main`에서 실행**해야 합니다.

`SIGNPATH_API_TOKEN`이 없으면 서명 단계는 건너뜁니다. 릴리스가 아닌 빌드(푸시, PR)는 언제나 서명하지 않습니다.

---

## Code signing policy

Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by [SignPath Foundation](https://signpath.org).
(This applies once the project is approved by SignPath Foundation.)

- **What is signed**: only `docx2hwpx-hlt.exe` and `docx2hwpx-hlt-gui.exe`, built from this repository by
  the GitHub-hosted `windows` workflow ([.github/workflows/windows.yml](.github/workflows/windows.yml)) for tagged releases.
  Third-party binaries are not signed.
- **Committers and reviewers**: [lmg6983-dotcom](https://github.com/lmg6983-dotcom) (repository owner).
  Changes from other contributors are merged only through reviewed pull requests.
- **Approvers**: [lmg6983-dotcom](https://github.com/lmg6983-dotcom). Every signing request is approved manually.
- **Privacy policy**: This program will not transfer any information to other networked systems.
  It works entirely offline; documents are read and written only on the user's computer.
