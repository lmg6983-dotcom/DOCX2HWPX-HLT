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
        p(t("【0001】 CO₂ 농도 10⁻³ mol/m³, 25 °C – 범위, x̂ 와 ŷ, Müller 법, 기호 "), '<w:r><w:sym w:font="Symbol" w:char="F062"/></w:r>', t(" 및 Hₙ.")),
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
    ]
)

ns = (
    'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" '
    'xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math" '
    'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" '
    'xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" '
    'xmlns:v="urn:schemas-microsoft-com:vml"'
)
document = f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {ns}><w:body>{body}<w:sectPr/></w:body></w:document>'
img = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image"
rels = (
    '<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
    f'<Relationship Id="rIdImg" Type="{img}" Target="media/image1.png"/>'
    f'<Relationship Id="rIdWmf" Type="{img}" Target="media/image2.wmf"/>'
    f'<Relationship Id="rIdEmf" Type="{img}" Target="media/image3.emf"/>'
    '<Relationship Id="rIdCm" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="comments.xml"/>'
    '<Relationship Id="rIdCmx" Type="http://schemas.microsoft.com/office/2011/relationships/commentsExtended" Target="commentsExtended.xml"/>'
    "</Relationships>"
)
types = (
    '<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
    '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
    '<Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/>'
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

root_rels = (
    '<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
    '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>'
)
out.parent.mkdir(exist_ok=True)
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
    z.writestr("[Content_Types].xml", types)
    z.writestr("_rels/.rels", root_rels)
    z.writestr("word/document.xml", document)
    z.writestr("word/_rels/document.xml.rels", rels)
    z.writestr("word/comments.xml", comments)
    z.writestr("word/commentsExtended.xml", comments_ex)
    z.writestr("word/media/image1.png", png(60, 40, (40, 120, 200)))
    z.write(here / "fixtures" / "shape.wmf", "word/media/image2.wmf")
    z.write(here / "fixtures" / "shape.emf", "word/media/image3.emf")
print(out)
