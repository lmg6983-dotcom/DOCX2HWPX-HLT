//! 작은 XML 트리. document.xml 을 읽어 고친 뒤 그대로 다시 쓰려고 둔다.
//! 이름은 접두사째(`w:r`) 들고 다닌다 — Word 는 접두사를 바꾸지 않는다.

use quick_xml::Reader;
use quick_xml::escape::{escape, unescape};
use quick_xml::events::Event;

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    El(Element),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

impl Element {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }
    pub fn attr(mut self, key: &str, value: &str) -> Self {
        self.attrs.push((key.into(), value.into()));
        self
    }
    pub fn child(mut self, child: Element) -> Self {
        self.children.push(Node::El(child));
        self
    }
    pub fn text_child(mut self, text: &str) -> Self {
        self.children.push(Node::Text(text.into()));
        self
    }
    pub fn get(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|n| match n {
            Node::El(e) => Some(e),
            Node::Text(_) => None,
        })
    }
    pub fn first(&self, name: &str) -> Option<&Element> {
        self.elements().find(|e| e.name == name)
    }
    /// 바로 아래 자식 `name` 의 `attr` 값(`<m:chr m:val="…"/>` 꼴).
    pub fn prop(&self, name: &str, attr: &str) -> Option<&str> {
        self.first(name)?.get(attr)
    }
    /// 이름이 `name` 인 첫 자손(깊이 우선).
    pub fn find(&self, name: &str) -> Option<&Element> {
        self.elements().find_map(|e| {
            if e.name == name {
                Some(e)
            } else {
                e.find(name)
            }
        })
    }
    /// 글자만 이어 붙인다.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for node in &self.children {
            match node {
                Node::Text(t) => out.push_str(t),
                Node::El(e) => out.push_str(&e.text()),
            }
        }
        out
    }
}

pub fn parse(bytes: &[u8]) -> Result<Element, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| format!("XML UTF-8: {e}"))?;
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(false);
    let mut stack: Vec<Element> = vec![Element::new("#root")];
    loop {
        let event = reader.read_event().map_err(|e| format!("XML: {e}"))?;
        match event {
            Event::Start(s) => stack.push(element(&reader, &s)?),
            Event::Empty(s) => {
                let el = element(&reader, &s)?;
                push(&mut stack, Node::El(el));
            }
            Event::End(_) => {
                let el = stack.pop().ok_or("XML: 짝 없는 끝 태그")?;
                push(&mut stack, Node::El(el));
            }
            Event::Text(t) => {
                let raw = t.decode().map_err(|e| format!("XML: {e}"))?;
                let s = unescape(&raw)
                    .map_err(|e| format!("XML: {e}"))?
                    .into_owned();
                push(&mut stack, Node::Text(s));
            }
            Event::CData(t) => {
                let s = t.decode().map_err(|e| format!("XML: {e}"))?.into_owned();
                push(&mut stack, Node::Text(s));
            }
            Event::GeneralRef(r) => {
                let name = r.decode().map_err(|e| format!("XML: {e}"))?;
                let s = unescape(&format!("&{name};"))
                    .map_err(|e| format!("XML: {e}"))?
                    .into_owned();
                push(&mut stack, Node::Text(s));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if stack.len() != 1 {
        return Err("XML: 닫히지 않은 태그".into());
    }
    stack
        .pop()
        .unwrap()
        .elements()
        .next()
        .cloned()
        .ok_or_else(|| "XML: 빈 문서".into())
}

fn element(_reader: &Reader<&[u8]>, s: &quick_xml::events::BytesStart) -> Result<Element, String> {
    let mut el = Element::new(std::str::from_utf8(s.name().as_ref()).map_err(|e| e.to_string())?);
    for a in s.attributes() {
        let a = a.map_err(|e| format!("XML attr: {e}"))?;
        let key = std::str::from_utf8(a.key.as_ref())
            .map_err(|e| e.to_string())?
            .to_string();
        let raw = String::from_utf8_lossy(&a.value);
        let value = unescape(&raw)
            .map_err(|e| format!("XML attr: {e}"))?
            .into_owned();
        el.attrs.push((key, value));
    }
    Ok(el)
}

fn push(stack: &mut [Element], node: Node) {
    let parent = stack.last_mut().expect("root");
    // 이웃한 글 조각은 합친다(엔티티가 글을 쪼갠다).
    if let (Node::Text(new), Some(Node::Text(last))) = (&node, parent.children.last_mut()) {
        last.push_str(new);
        return;
    }
    parent.children.push(node);
}

pub fn write(root: &Element) -> Vec<u8> {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n");
    write_el(root, &mut out);
    out.into_bytes()
}

fn write_el(el: &Element, out: &mut String) {
    out.push('<');
    out.push_str(&el.name);
    for (k, v) in &el.attrs {
        out.push_str(&format!(
            " {k}=\"{}\"",
            escape(v.as_str()).replace('"', "&quot;")
        ));
    }
    if el.children.is_empty() {
        out.push_str("/>");
        return;
    }
    out.push('>');
    for node in &el.children {
        match node {
            Node::Text(t) => out.push_str(&escape(t.as_str())),
            Node::El(e) => write_el(e, out),
        }
    }
    out.push_str(&format!("</{}>", el.name));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_keeps_text_attrs_and_empty() {
        let src = br#"<?xml version="1.0"?><w:d a="1 &amp; 2"><w:t xml:space="preserve"> a &lt; b </w:t><w:br/></w:d>"#;
        let root = parse(src).unwrap();
        assert_eq!(root.get("a"), Some("1 & 2"));
        assert_eq!(root.text(), " a < b ");
        assert!(root.first("w:br").unwrap().children.is_empty());
        let again = parse(&write(&root)).unwrap();
        assert_eq!(again, root);
    }
}
