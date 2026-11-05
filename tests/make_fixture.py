"""시험용 DOCX(tests/fixtures/sample.docx)를 만든다: 수식 · 합친 표 · 표 안 표 · 그림(PNG · WMF · EMF) · 특수 글자 ·
메모 · 변경 추적(넣기 · 지우기 · 옮기기 · 서식 변경 · 지운 문단 표시 · 지운 줄).
python3 tests/make_fixture.py  (표준 라이브러리만 쓴다)"""
import struct, zlib, zipfile, pathlib

here = pathlib.Path(__file__).parent
out = here / "fixtures" / "sample.docx"


def png(w, h, rgb):
    raw = b"".join(b"\x00" + bytes(rgb) * w for _ in range(h))
    chunk = lambda t, d: struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d))
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")


def t(s):
    return f'<w:r><w:t xml:space="preserve">{s}</w:t></w:r>'


def sub(s):
    return f'<w:r><w:rPr><w:vertAlign w:val="subscript"/></w:rPr><w:t xml:space="preserve">{s}</w:t></w:r>'


def p(*runs, jc=None):
    ppr = f'<w:pPr><w:jc w:val="{jc}"/></w:pPr>' if jc else ""
    return f"<w:p>{ppr}{''.join(runs)}</w:p>"


def mr(s, sty=None):
    rpr = f'<m:rPr><m:sty m:val="{sty}"/></m:rPr>' if sty else ""
    return f"<m:r>{rpr}<m:t>{s}</m:t></m:r>"


def e(*xs):
    return f"<m:e>{''.join(xs)}</m:e>"


def ssup(base, sup):
    return f"<m:sSup>{e(base)}<m:sup>{sup}</m:sup></m:sSup>"


def ssub(base, sub):
    return f"<m:sSub>{e(base)}<m:sub>{sub}</m:sub></m:sSub>"


def frac(n, d):
    return f"<m:f><m:num>{n}</m:num><m:den>{d}</m:den></m:f>"


def omath(*xs):
    return f"<m:oMath>{''.join(xs)}</m:oMath>"


def display(*xs):
    return f"<w:p><m:oMathPara>{omath(*xs)}</m:oMathPara></w:p>"


def drawing(rid, cx, cy, n):
    return (
        f'<w:r><w:drawing><wp:inline><wp:extent cx="{cx}" cy="{cy}"/><wp:docPr id="{n}" name="p{n}"/>'
        '<a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">'
        f'<pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:nvPicPr><pic:cNvPr id="{n}" name="p{n}"/><pic:cNvPicPr/></pic:nvPicPr>'
        f'<pic:blipFill><a:blip r:embed="{rid}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>'
        f'<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic>'
        "</a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"
    )


def group(n):
    """ChemDraw 를 「도형으로 변환」한 꼴: 그룹 안에 그림 · 벡터 경로(고리) · 화살표 · 글상자(NaBH₄)."""
    hexagon = "".join(
        f'<a:{"moveTo" if i == 0 else "lnTo"}><a:pt x="{x}" y="{y}"/></a:{"moveTo" if i == 0 else "lnTo"}>'
        for i, (x, y) in enumerate([(50, 0), (100, 25), (100, 75), (50, 100), (0, 75), (0, 25)])
    )
    ln = '<a:ln w="12700"><a:solidFill><a:srgbClr val="000000"/></a:solidFill>{}</a:ln>'
    return (
        f'<w:r><w:drawing><wp:inline><wp:extent cx="2400000" cy="800000"/><wp:docPr id="{n}" name="Group {n}"/>'
        '<a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup">'
        '<wpg:wgp><wpg:cNvGrpSpPr/><wpg:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="2400000" cy="800000"/>'
        '<a:chOff x="0" y="0"/><a:chExt cx="4800000" cy="1600000"/></a:xfrm></wpg:grpSpPr>'
        '<pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:nvPicPr><pic:cNvPr id="90" name="part"/><pic:cNvPicPr/></pic:nvPicPr>'
        '<pic:blipFill><a:blip r:embed="rIdImg"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>'
        '<pic:spPr><a:xfrm><a:off x="3400000" y="200000"/><a:ext cx="1200000" cy="1200000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic>'
        '<wps:wsp><wps:cNvPr id="91" name="ring"/><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="200000" y="200000"/><a:ext cx="1200000" cy="1200000"/></a:xfrm>'
        f'<a:custGeom><a:pathLst><a:path w="100" h="100">{hexagon}<a:close/></a:path></a:pathLst></a:custGeom>{ln.format("")}</wps:spPr><wps:bodyPr/></wps:wsp>'
        '<wps:wsp><wps:cNvPr id="92" name="arrow"/><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="1700000" y="800000"/><a:ext cx="1400000" cy="0"/></a:xfrm>'
        f'<a:prstGeom prst="straightConnector1"><a:avLst/></a:prstGeom>{ln.format(chr(60) + "a:tailEnd type=" + chr(34) + "triangle" + chr(34) + "/>")}</wps:spPr><wps:bodyPr/></wps:wsp>'
        '<wps:wsp><wps:cNvPr id="93" name="reagent"/><wps:cNvSpPr txBox="1"/><wps:spPr><a:xfrm><a:off x="1700000" y="300000"/><a:ext cx="1400000" cy="400000"/></a:xfrm>'
        '<a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/></wps:spPr><wps:txbx><w:txbxContent><w:p><w:pPr><w:jc w:val="center"/></w:pPr>'
        '<w:r><w:rPr><w:sz w:val="20"/></w:rPr><w:t>NaBH</w:t></w:r><w:r><w:rPr><w:sz w:val="20"/><w:vertAlign w:val="subscript"/></w:rPr><w:t>4</w:t></w:r></w:p></w:txbxContent></wps:txbx>'
        '<wps:bodyPr lIns="0" tIns="0" rIns="0" bIns="0"/></wps:wsp>'
        "</wpg:wgp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"
    )


def tc(content, w, span=None, vmerge=None):
    pr = f'<w:tcW w:w="{w}" w:type="dxa"/>'
    if span:
        pr += f'<w:gridSpan w:val="{span}"/>'
    if vmerge is not None:
        pr += '<w:vMerge w:val="restart"/>' if vmerge == "restart" else "<w:vMerge/>"
    return f"<w:tc><w:tcPr>{pr}</w:tcPr>{content}</w:tc>"


def tbl(grid, rows):
    g = "".join(f'<w:gridCol w:w="{w}"/>' for w in grid)
    return f'<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid>{g}</w:tblGrid>{"".join(f"<w:tr>{r}</w:tr>" for r in rows)}</w:tbl>'


inner = tbl([1200, 1200], [tc(p(t("안1")), 1200) + tc(p(t("안2")), 1200)])
equations = [
    # 근의 공식
    display(mr("x"), mr("="), frac(mr("-b±") + f"<m:rad><m:radPr><m:degHide m:val=\"1\"/></m:radPr><m:deg/>{e(ssup(mr('b'), mr('2')), mr('-4ac'))}</m:rad>", mr("2a"))),
    # 합 · 적분 · 극한 · 함수
    display(
        f'<m:nary><m:naryPr><m:chr m:val="∑"/></m:naryPr><m:sub>{mr("i=1")}</m:sub><m:sup>{mr("n")}</m:sup>{e(ssub(mr("a"), mr("i")))}</m:nary>',
        mr("+"),
        f'<m:nary><m:sub>{mr("0")}</m:sub><m:sup>{mr("∞")}</m:sup>{e(ssup(mr("e"), mr("-x")), mr("dx"))}</m:nary>',
        mr("+"),
        f'<m:func><m:fName><m:limLow>{e(mr("lim", "p"))}<m:lim>{mr("n→∞")}</m:lim></m:limLow></m:fName>{e(mr("f"), "<m:d>" + e(mr("n")) + "</m:d>")}</m:func>',
        mr("+"),
        f'<m:func><m:fName>{mr("sin", "p")}</m:fName>{e(mr("θ"))}</m:func>',
    ),
    # 행렬 · 괄호 · 악센트 · 윗줄 · 수식 배열 · 한쪽 괄호
    display(
        '<m:d><m:dPr><m:begChr m:val="["/><m:endChr m:val="]"/></m:dPr>' + e('<m:m><m:mr>' + e(mr("1")) + e(mr("0")) + "</m:mr><m:mr>" + e(mr("0")) + e(mr("1")) + "</m:mr></m:m>") + "</m:d>",
        f'<m:acc>{e(mr("y"))}</m:acc>',
        f'<m:bar>{e(mr("AB"))}</m:bar>',
        mr("≤"),
        f'<m:acc><m:accPr><m:chr m:val="⃗"/></m:accPr>{e(mr("v"))}</m:acc>',
    ),
    display(
        '<m:d><m:dPr><m:begChr m:val="{"/><m:endChr m:val=""/></m:dPr>'
        + e("<m:eqArr>" + e(mr("x,&amp; x≥0")) + e(mr("-x,&amp; x&lt;0")) + "</m:eqArr>")
        + "</m:d>"
    ),
]

body = "".join(
    [
        p(t("【발명의 설명】")),
        p(t("【발명의 명칭】")),
        p(t("시험용 발명")),
        p(t("【기술분야】")),
        p(t("【0001】 CO₂ 농도 10⁻³ mol/m³, 25 °C – 범위, x̂ 와 ŷ, Müller 법, 기호 "), '<w:r><w:sym w:font="Symbol" w:char="F062"/></w:r>', t(" 및 Hₙ. "),
          # Symbol 글꼴 글(a → α · Ò → ® · U+F0B4 → ×), 줄 안 나뉘는 하이픈, 풀어쓴 한글, 일본어 가운뎃점
          '<w:r><w:rPr><w:rFonts w:ascii="Symbol" w:hAnsi="Symbol"/></w:rPr><w:t>a\u00d2\uf0b4</w:t></w:r>',
          '<w:r><w:t>1</w:t><w:noBreakHyphen/><w:t>2</w:t></w:r>', t(" \u1112\u1161\u11ab\u30fb끝 "),
          # 글자 스타일로만 받은 Symbol 글꼴(b → β), MT Extra(h → ℏ), ≈ → ≒
          '<w:r><w:rPr><w:rStyle w:val="SymChar"/></w:rPr><w:t>b</w:t></w:r>',
          '<w:r><w:rPr><w:rFonts w:ascii="MT Extra" w:hAnsi="MT Extra"/></w:rPr><w:t>h</w:t></w:r>', t(" ≈ 1")),
        p(t("【0002】 식 "), omath(ssup(mr("x"), mr("2")), mr("+"), ssub(mr("y"), mr("1"))), t(" 은 본문 수식이다. 수정 "), '<w:ins w:id="1" w:author="a">' + t("넣은 글") + "</w:ins>", '<w:del w:id="2" w:author="a"><w:r><w:delText>지운 글</w:delText></w:r></w:del>', t(".")),
        *equations,
        p(t("【0003】 표는 다음과 같다.")),
        tbl(
            [1800, 1800, 1800],
            [
                tc(p(t("합친 머리")), 3600, span=2) + tc(p(t("세로 합침")), 1800, vmerge="restart"),
                tc(p(t("첫 줄")) + p(t("둘째 줄")), 1800) + tc(p(t("수식 ")) + p(omath(frac(mr("a"), mr("b")))), 1800) + tc(p(), 1800, vmerge="continue"),
                tc(p(t("그림")) + p(drawing("rIdImg", 540000, 360000, 2)), 1800) + tc(inner + p(), 1800) + tc(p(t("끝")), 1800),
                # 변경 추적으로 지운 줄
                '<w:trPr><w:del w:id="9" w:author="a"/></w:trPr>' + tc(p(t("지운 줄")), 3600, span=2) + tc(p(t("지운 줄")), 1800),
            ],
        ),
        # hlt-core 가 거부하던 표: 소수 열 너비 · 줄 앞 빈 자리 · 칸이 모자란 줄 · 위가 없는 세로 병합 · 콘텐츠 컨트롤 칸
        '<w:tbl><w:tblPr><w:tblInd w:w="100.5" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="1800.4"/><w:gridCol w:w="1800"/><w:gridCol/></w:tblGrid>'
        f'<w:tr>{tc(p(t("화합물")), 1800, vmerge="continue")}{tc(p(t("구조")), 1800)}{tc(p(t("수율")), 1800)}</w:tr>'
        f'<w:tr>{tc(p(), 1800, vmerge="continue")}<w:sdt><w:sdtPr/><w:sdtContent>{tc(p(t("화합물 A")), 1800)}</w:sdtContent></w:sdt></w:tr>'
        f'<w:tr><w:trPr><w:gridBefore w:val="1"/></w:trPr>{tc(p(t("화합물 B")), 1800)}{tc(p(t("92%")), 1800)}</w:tr></w:tbl>',
        p(t("【0004】 WMF 와 EMF 그림: "), drawing("rIdWmf", 1800000, 900000, 3), drawing("rIdEmf", 1800000, 900000, 4)),
        # 옛 OLE 수식(Equation 3.0 · MathType): VML 에 WMF 미리보기만 있다
        p(t("【0005】 OLE 수식 "), '<w:r><w:object><v:shape style="width:60pt;height:20pt"><v:imagedata r:id="rIdWmf"/></v:shape></w:object></w:r>', t(" 끝.")),
        # 메모 · 서식 변경
        p(
            t("【0006】 메모 "),
            '<w:commentRangeStart w:id="0"/>',
            t("달린 글"),
            '<w:commentRangeEnd w:id="0"/>',
            '<w:r><w:rPr><w:rStyle w:val="CommentReference"/></w:rPr><w:commentReference w:id="0"/></w:r>',
            '<w:r><w:rPr><w:b/><w:rPrChange w:id="10" w:author="a"><w:rPr/></w:rPrChange></w:rPr><w:t xml:space="preserve"> 굵게 바뀐 글</w:t></w:r>',
        ),
        # 지운 문단 표시: 다음 문단과 합쳐진다
        '<w:p><w:pPr><w:rPr><w:del w:id="11" w:author="a"/></w:rPr></w:pPr>' + t("【0007】 앞 문단") + "</w:p>",
        p(t(" 뒷 문단")),
        # 옮기기
        p(t("【0008】 "), '<w:moveFrom w:id="12" w:author="a">' + t("옮겨 간 글") + "</w:moveFrom>", t("가운데"), '<w:moveTo w:id="13" w:author="a">' + t(" 옮겨 온 글") + "</w:moveTo>"),
        # 긴 첨자 단어가 여러 번 든 긴 문단 — HLT 저장 줄이 첨자 · 영문 낱말 가운데서 끊기지 않아야 한다
        p(
            *[
                piece
                for _ in range(3)
                for piece in (
                    t("상자 정보는 좌표 묶음으로 나타낼 수 있으며, 일 예로 [(x"), sub("min"), t(", y"), sub("min"), t(", x"), sub("max"),
                    t(", y"), sub("max"), t(", class"), sub("identifier"), t(", confidence"), sub("score"),
                    t("), …]의 형태로 표현되고 기본 점수(Anomaly"), sub("default"), t(")가 함께 저장될 수 있다. "),
                )
            ]
        ),
        p('<w:r><w:br w:type="page"/></w:r>'),
        p(t("【청구범위】")),
        p(t("【청구항 1】")),
        p(t("A 와 B 를 포함하는 장치.")),
        p(t("【요약서】")),
        p(t("【요약】")),
        p(t("시험 요약.")),
        p(t("【도면】")),
        p(t("【도 1】")),
        p(drawing("rIdImg", 1800000, 1200000, 5)),
        # SVG 그림(Word 365 아이콘) · 링크로만 연결된 그림 — hlt-core 가 거부하던 그림 관계
        p(t("【0009】 SVG "), drawing("rIdSvg", 900000, 450000, 7), t(" 와 링크 그림 "),
          drawing("rIdLinked", 900000, 600000, 8).replace('r:embed="rIdLinked"', 'r:link="rIdLinked"')),
        p(t("【도 2】")),
        p(group(6)),
    ]
)

ns = (
    'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" '
    'xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math" '
    'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" '
    'xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" '
    'xmlns:v="urn:schemas-microsoft-com:vml" '
    'xmlns:wpg="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup" '
    'xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"'
)
document = f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {ns}><w:body>{body}<w:sectPr/></w:body></w:document>'
img = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image"
rels = (
    '<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
    '<Relationship Id="rIdSty" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>'
    f'<Relationship Id="rIdImg" Type="{img}" Target="media/image1.png"/>'
    f'<Relationship Id="rIdSvg" Type="{img}" Target="media/icon.svg"/>'
    f'<Relationship Id="rIdLinked" Type="{img}" Target="file:///C:/figs/fig1.png" TargetMode="External"/>'
    # 파일이 없는 관계(본문은 쓰지 않는다) — 조용히 지운다
    f'<Relationship Id="rIdGone" Type="{img}" Target="media/gone.png"/>'
    f'<Relationship Id="rIdWmf" Type="{img}" Target="media/image2.wmf"/>'
    f'<Relationship Id="rIdEmf" Type="{img}" Target="media/image3.emf"/>'
    '<Relationship Id="rIdCm" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="comments.xml"/>'
    '<Relationship Id="rIdCmx" Type="http://schemas.microsoft.com/office/2011/relationships/commentsExtended" Target="commentsExtended.xml"/>'
    "</Relationships>"
)
types = (
    '<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
    '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
    '<Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Default Extension="svg" ContentType="image/svg+xml"/>'
    '<Default Extension="wmf" ContentType="image/x-wmf"/><Default Extension="emf" ContentType="image/x-emf"/>'
    '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
    '<Override PartName="/word/comments.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml"/>'
    '<Override PartName="/word/commentsExtended.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.commentsExtended+xml"/></Types>'
)
comments = (
    '<?xml version="1.0" encoding="UTF-8"?><w:comments xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">'
    '<w:comment w:id="0" w:author="검토자"><w:p><w:r><w:t>지워야 할 메모</w:t></w:r></w:p></w:comment></w:comments>'
)
comments_ex = '<?xml version="1.0" encoding="UTF-8"?><w15:commentsEx xmlns:w15="http://schemas.microsoft.com/office/word/2012/wordml"/>'

styles = (
    '<?xml version="1.0" encoding="UTF-8"?><w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">'
    '<w:style w:type="character" w:styleId="SymBase"><w:name w:val="Sym Base"/><w:rPr><w:rFonts w:ascii="Symbol" w:hAnsi="Symbol"/></w:rPr></w:style>'
    '<w:style w:type="character" w:styleId="SymChar"><w:name w:val="Sym Char"/><w:basedOn w:val="SymBase"/></w:style></w:styles>'
)

root_rels = (
    '<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
    '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>'
)
out.parent.mkdir(exist_ok=True)
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
    z.writestr("[Content_Types].xml", types)
    z.writestr("_rels/.rels", root_rels)
    z.writestr("word/document.xml", document)
    z.writestr("word/styles.xml", styles)
    z.writestr("word/_rels/document.xml.rels", rels)
    z.writestr("word/comments.xml", comments)
    z.writestr("word/commentsExtended.xml", comments_ex)
    z.writestr("word/media/image1.png", png(60, 40, (40, 120, 200)))
    z.writestr(
        "word/media/icon.svg",
        '<svg xmlns="http://www.w3.org/2000/svg" width="80" height="40"><rect x="2" y="2" width="76" height="36" fill="none" stroke="black" stroke-width="3"/></svg>',
    )
    z.write(here / "fixtures" / "shape.wmf", "word/media/image2.wmf")
    z.write(here / "fixtures" / "shape.emf", "word/media/image3.emf")
print(out)
