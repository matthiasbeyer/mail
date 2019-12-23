use std::collections::BTreeMap;

use charset::decode_latin1;
use charset::Charset;

use crate::body::Body;
use crate::util::*;
use crate::error::*;
use crate::header::*;

/// A struct to hold a more structured representation of the Content-Type header.
/// This is provided mostly as a convenience since this metadata is usually
/// needed to interpret the message body properly.
#[derive(Debug)]
pub struct ParsedContentType {
    /// The type of the data, for example "text/plain" or "application/pdf".
    pub mimetype: String,
    /// The charset used to decode the raw byte data, for example "iso-8859-1"
    /// or "utf-8".
    pub charset: String,
    /// The additional params of Content-Type, e.g. filename and boundary. The
    /// keys in the map will be lowercased, and the values will have any
    /// enclosing quotes stripped.
    pub params: BTreeMap<String, String>,
}

impl Default for ParsedContentType {
    fn default() -> Self {
        ParsedContentType {
            mimetype: "text/plain".to_string(),
            charset: "us-ascii".to_string(),
            params: BTreeMap::new(),
        }
    }
}

/// Helper method to parse a header value as a Content-Type header. Note that
/// the returned object's `params` map will contain a charset key if a charset
/// was explicitly specified in the header; otherwise the `params` map will not
/// contain a charset key. Regardless, the `charset` field will contain a
/// charset - either the one explicitly specified or the default of "us-ascii".
///
/// # Examples
/// ```
///     use header::parse_header;
///     use parser::parse_content_type;
///     let (parsed, _) = parse_header(
///             b"Content-Type: text/html; charset=foo; boundary=\"quotes_are_removed\"")
///         .unwrap();
///     let ctype = parse_content_type(&parsed.get_value().unwrap());
///     assert_eq!(ctype.mimetype, "text/html");
///     assert_eq!(ctype.charset, "foo");
///     assert_eq!(ctype.params.get("boundary"), Some(&"quotes_are_removed".to_string()));
///     assert_eq!(ctype.params.get("charset"), Some(&"foo".to_string()));
/// ```
/// ```
///     use header::parse_header;
///     use parser::parse_content_type;
///     let (parsed, _) = parse_header(b"Content-Type: bogus").unwrap();
///     let ctype = parse_content_type(&parsed.get_value().unwrap());
///     assert_eq!(ctype.mimetype, "bogus");
///     assert_eq!(ctype.charset, "us-ascii");
///     assert_eq!(ctype.params.get("boundary"), None);
///     assert_eq!(ctype.params.get("charset"), None);
/// ```
/// ```
///     use header::parse_header;
///     use parser::parse_content_type;
///     let (parsed, _) = parse_header(br#"Content-Type: application/octet-stream;name="=?utf8?B?6L+O5ai255m95a+M576O?=";charset="utf8""#).unwrap();
///     let ctype = parse_content_type(&parsed.get_value().unwrap());
///     assert_eq!(ctype.mimetype, "application/octet-stream");
///     assert_eq!(ctype.charset, "utf8");
///     assert_eq!(ctype.params.get("boundary"), None);
///     assert_eq!(ctype.params.get("name"), Some(&"迎娶白富美".to_string()));
/// ```
pub fn parse_content_type(header: &str) -> ParsedContentType {
    let params = parse_param_content(header);
    let mimetype = params.value.to_lowercase();
    let charset = params
        .params
        .get("charset")
        .cloned()
        .unwrap_or_else(|| "us-ascii".to_string());

    ParsedContentType {
        mimetype,
        charset,
        params: params.params,
    }
}

/// The possible disposition types in a Content-Disposition header. A more
/// comprehensive list of IANA-recognized types can be found at
/// https://www.iana.org/assignments/cont-disp/cont-disp.xhtml. This library
/// only enumerates the types most commonly found in email messages, and
/// provides the `Extension` value for holding all other types.
#[derive(Debug, Clone, PartialEq)]
pub enum DispositionType {
    /// Default value, indicating the content is to be displayed inline as
    /// part of the enclosing document.
    Inline,
    /// A disposition indicating the content is not meant for inline display,
    /// but whose content can be accessed for use.
    Attachment,
    /// A disposition indicating the content contains a form submission.
    FormData,
    /// Extension type to hold any disposition not explicitly enumerated.
    Extension(String),
}

impl Default for DispositionType {
    fn default() -> Self {
        DispositionType::Inline
    }
}

/// Convert the string represented disposition type to enum.
fn parse_disposition_type(disposition: &str) -> DispositionType {
    match &disposition.to_lowercase()[..] {
        "inline" => DispositionType::Inline,
        "attachment" => DispositionType::Attachment,
        "form-data" => DispositionType::FormData,
        extension => DispositionType::Extension(extension.to_string()),
    }
}

/// A struct to hold a more structured representation of the Content-Disposition header.
/// This is provided mostly as a convenience since this metadata is usually
/// needed to interpret the message body properly.
#[derive(Debug, Default)]
pub struct ParsedContentDisposition {
    /// The disposition type of the Content-Disposition header. If this
    /// is an extension type, the string will be lowercased.
    pub disposition: DispositionType,
    /// The additional params of Content-Disposition, e.g. filename. The
    /// keys in the map will be lowercased, and the values will have any
    /// enclosing quotes stripped.
    pub params: BTreeMap<String, String>,
}

/// Helper method to parse a header value as a Content-Disposition header. The disposition
/// defaults to "inline" if no disposition parameter is provided in the header
/// value.
///
/// # Examples
/// ```
///     use header::parse_header;
///     use parser::{parse_content_disposition, DispositionType};
///     let (parsed, _) = parse_header(
///             b"Content-Disposition: attachment; filename=\"yummy dummy\"")
///         .unwrap();
///     let dis = parse_content_disposition(&parsed.get_value().unwrap());
///     assert_eq!(dis.disposition, DispositionType::Attachment);
///     assert_eq!(dis.params.get("name"), None);
///     assert_eq!(dis.params.get("filename"), Some(&"yummy dummy".to_string()));
/// ```
pub fn parse_content_disposition(header: &str) -> ParsedContentDisposition {
    let params = parse_param_content(header);
    let disposition = parse_disposition_type(&params.value);
    ParsedContentDisposition {
        disposition,
        params: params.params,
    }
}

/// Struct that holds the structured representation of the message. Note that
/// since MIME allows for nested multipart messages, a tree-like structure is
/// necessary to represent it properly. This struct accomplishes that by holding
/// a vector of other ParsedMail structures for the subparts.
#[derive(Debug)]
pub struct ParsedMail<'a> {
    /// The headers for the message (or message subpart).
    pub headers: Vec<MailHeader<'a>>,
    /// The Content-Type information for the message (or message subpart).
    pub ctype: ParsedContentType,
    /// The raw bytes that make up the body of the message (or message subpart).
    body: &'a [u8],
    /// The subparts of this message or subpart. This vector is only non-empty
    /// if ctype.mimetype starts with "multipart/".
    pub subparts: Vec<ParsedMail<'a>>,
}

impl<'a> ParsedMail<'a> {
    /// Get the body of the message as a Rust string. This function tries to
    /// unapply the Content-Transfer-Encoding if there is one, and then converts
    /// the result into a Rust UTF-8 string using the charset in the Content-Type
    /// (or "us-ascii" if the charset was missing or not recognized).
    ///
    /// # Examples
    /// ```
    ///     use parser::parse_mail;
    ///     let p = parse_mail(concat!(
    ///             "Subject: test\n",
    ///             "\n",
    ///             "This is the body").as_bytes())
    ///         .unwrap();
    ///     assert_eq!(p.get_body().unwrap(), "This is the body");
    /// ```
    pub fn get_body(&self) -> Result<String, MailParseError> {
        match self.get_body_encoded()? {
            Body::Base64(body) | Body::QuotedPrintable(body) => body.get_decoded_as_string(),
            Body::SevenBit(body) | Body::EightBit(body) => body.get_as_string(),
            Body::Binary(_) => Err(MailParseError::Generic(
                "Message body of type binary body cannot be parsed into a string",
            )),
        }
    }

    /// Get the body of the message as a Rust Vec<u8>. This function tries to
    /// unapply the Content-Transfer-Encoding if there is one, but won't do
    /// any charset decoding.
    ///
    /// # Examples
    /// ```
    ///     use parser::parse_mail;
    ///     let p = parse_mail(concat!(
    ///             "Subject: test\n",
    ///             "\n",
    ///             "This is the body").as_bytes())
    ///         .unwrap();
    ///     assert_eq!(p.get_body_raw().unwrap(), b"This is the body");
    /// ```
    pub fn get_body_raw(&self) -> Result<Vec<u8>, MailParseError> {
        match self.get_body_encoded()? {
            Body::Base64(body) | Body::QuotedPrintable(body) => body.get_decoded(),
            Body::SevenBit(body) | Body::EightBit(body) => Ok(Vec::<u8>::from(body.get_raw())),
            Body::Binary(body) => Ok(Vec::<u8>::from(body.get_raw())),
        }
    }

    /// Get the body of the message.
    /// This function returns original the body without attempting to
    /// unapply the Content-Transfer-Encoding.
    ///
    /// # Examples
    /// ```
    ///     use parser::parse_mail;
    ///     use body::Body;
    ///
    ///     let mail = parse_mail(b"Content-Transfer-Encoding: base64\r\n\r\naGVsbG 8gd\r\n29ybGQ=").unwrap();
    ///
    ///     match mail.get_body_encoded().unwrap() {
    ///         Body::Base64(body) => {
    ///             assert_eq!(body.get_raw(), b"aGVsbG 8gd\r\n29ybGQ=");
    ///             assert_eq!(body.get_decoded().unwrap(), b"hello world");
    ///             assert_eq!(body.get_decoded_as_string().unwrap(), "hello world");
    ///         },
    ///         _ => assert!(false),
    ///     };
    ///
    ///
    ///     // An email whose body encoding is not known upfront
    ///     let another_mail = parse_mail(b"").unwrap();
    ///
    ///     match another_mail.get_body_encoded().unwrap() {
    ///         Body::Base64(body) | Body::QuotedPrintable(body) => {
    ///             println!("mail body encoded: {:?}", body.get_raw());
    ///             println!("mail body decoded: {:?}", body.get_decoded().unwrap());
    ///             println!("mail body decoded as string: {}", body.get_decoded_as_string().unwrap());
    ///         },
    ///         Body::SevenBit(body) | Body::EightBit(body) => {
    ///             println!("mail body: {:?}", body.get_raw());
    ///             println!("mail body as string: {}", body.get_as_string().unwrap());
    ///         },
    ///         Body::Binary(body) => {
    ///             println!("mail body binary: {:?}", body.get_raw());
    ///         }
    ///     }
    /// ```
    pub fn get_body_encoded(&'a self) -> Result<Body<'a>, MailParseError> {
        let transfer_encoding = self
            .headers
            .get_first_value("Content-Transfer-Encoding")?
            .map(|s| s.to_lowercase());

        Ok(Body::new(self.body, &self.ctype, &transfer_encoding))
    }

    /// Returns a struct containing a parsed representation of the
    /// Content-Disposition header. The first header with this name
    /// is used, if there are multiple. See the `parse_content_disposition`
    /// method documentation for more details on the semantics of the
    /// returned object.
    pub fn get_content_disposition(&self) -> Result<ParsedContentDisposition, MailParseError> {
        let disposition = self
            .headers
            .get_first_value("Content-Disposition")?
            .map(|s| parse_content_disposition(&s))
            .unwrap_or_default();
        Ok(disposition)
    }
}

/// The main mail-parsing entry point.
/// This function takes the raw data making up the message body and returns a
/// structured version of it, which allows easily accessing the header and body
/// information as needed.
///
/// # Examples
/// ```
///     use parser::*;
///     let parsed = parse_mail(concat!(
///             "Subject: This is a test email\n",
///             "Content-Type: multipart/alternative; boundary=foobar\n",
///             "Date: Sun, 02 Oct 2016 07:06:22 -0700 (PDT)\n",
///             "\n",
///             "--foobar\n",
///             "Content-Type: text/plain; charset=utf-8\n",
///             "Content-Transfer-Encoding: quoted-printable\n",
///             "\n",
///             "This is the plaintext version, in utf-8. Proof by Euro: =E2=82=AC\n",
///             "--foobar\n",
///             "Content-Type: text/html\n",
///             "Content-Transfer-Encoding: base64\n",
///             "\n",
///             "PGh0bWw+PGJvZHk+VGhpcyBpcyB0aGUgPGI+SFRNTDwvYj4gdmVyc2lvbiwgaW4g \n",
///             "dXMtYXNjaWkuIFByb29mIGJ5IEV1cm86ICZldXJvOzwvYm9keT48L2h0bWw+Cg== \n",
///             "--foobar--\n",
///             "After the final boundary stuff gets ignored.\n").as_bytes())
///         .unwrap();
///     assert_eq!(parsed.headers.get_first_value("Subject").unwrap(),
///         Some("This is a test email".to_string()));
///     assert_eq!(parsed.subparts.len(), 2);
///     assert_eq!(parsed.subparts[0].get_body().unwrap(),
///         "This is the plaintext version, in utf-8. Proof by Euro: \u{20AC}");
///     assert_eq!(parsed.subparts[1].headers[1].get_value().unwrap(), "base64");
///     assert_eq!(parsed.subparts[1].ctype.mimetype, "text/html");
///     assert!(parsed.subparts[1].get_body().unwrap().starts_with("<html>"));
///     assert_eq!(dateparse(parsed.headers.get_first_value("Date").unwrap().unwrap().as_str()).unwrap(), 1475417182);
/// ```
pub fn parse_mail(raw_data: &[u8]) -> Result<ParsedMail, MailParseError> {
    let (headers, ix_body) = parse_headers(raw_data)?;
    let ctype = headers
        .get_first_value("Content-Type")?
        .map(|s| parse_content_type(&s))
        .unwrap_or_default();

    let mut result = ParsedMail {
        headers,
        ctype,
        body: &raw_data[ix_body..],
        subparts: Vec::<ParsedMail>::new(),
    };
    if result.ctype.mimetype.starts_with("multipart/")
        && result.ctype.params.get("boundary").is_some()
        && raw_data.len() > ix_body
    {
        let boundary = String::from("--") + &result.ctype.params["boundary"];
        if let Some(ix_body_end) = find_from_u8(raw_data, ix_body, boundary.as_bytes()) {
            result.body = &raw_data[ix_body..ix_body_end];
            let mut ix_boundary_end = ix_body_end + boundary.len();
            while let Some(ix_part_start) =
                find_from_u8(raw_data, ix_boundary_end, b"\n").map(|v| v + 1)
            {
                // if there is no terminating boundary, assume the part end is the end of the email
                let ix_part_end = find_from_u8(raw_data, ix_part_start, boundary.as_bytes())
                    .unwrap_or_else(|| raw_data.len());

                result
                    .subparts
                    .push(parse_mail(&raw_data[ix_part_start..ix_part_end])?);
                ix_boundary_end = ix_part_end + boundary.len();
                if ix_boundary_end + 2 > raw_data.len()
                    || (raw_data[ix_boundary_end] == b'-' && raw_data[ix_boundary_end + 1] == b'-')
                {
                    break;
                }
            }
        }
    }
    Ok(result)
}

/// Used to store params for content-type and content-disposition
struct ParamContent {
    value: String,
    params: BTreeMap<String, String>,
}

/// Parse parameterized header values such as that for Content-Type
/// e.g. `multipart/alternative; boundary=foobar`
/// Note: this function is not made public as it may require
/// significant changes to be fully correct. For instance,
/// it does not handle quoted parameter values containing the
/// semicolon (';') character. It also produces a BTreeMap,
/// which implicitly does not support multiple parameters with
/// the same key. The format for parameterized header values
/// doesn't appear to be strongly specified anywhere.
fn parse_param_content(content: &str) -> ParamContent {
    let mut tokens = content.split(';');
    // There must be at least one token produced by split, even if it's empty.
    let value = tokens.next().unwrap().trim();
    let map = tokens
        .filter_map(|kv| {
            kv.find('=').map(|idx| {
                let key = kv[0..idx].trim().to_lowercase();
                let mut value = kv[idx + 1..].trim();
                if value.starts_with('"') && value.ends_with('"') && value.len() > 1 {
                    value = &value[1..value.len() - 1];
                }
                (key, value.to_string())
            })
        })
        .collect();

    ParamContent {
        value: value.into(),
        params: map,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dateparse::dateparse;

    macro_rules! assert_match {
        ( $x:expr, $p:pat ) => {
            match $x {
                $p => (),
                _ => panic!(
                    "Expression {} does not match pattern {}",
                    $x,
                    stringify!($p)
                ),
            }
        };
    }

    #[test]
    fn parse_basic_header() {
        let (parsed, _) = parse_header(b"Key: Value").unwrap();
        assert_eq!(parsed.key, b"Key");
        assert_eq!(parsed.get_key().unwrap(), "Key");
        assert_eq!(parsed.value, b"Value");
        assert_eq!(parsed.get_value().unwrap(), "Value");

        let (parsed, _) = parse_header(b"Key :  Value ").unwrap();
        assert_eq!(parsed.key, b"Key ");
        assert_eq!(parsed.value, b"Value ");
        assert_eq!(parsed.get_value().unwrap(), "Value ");

        let (parsed, _) = parse_header(b"Key:").unwrap();
        assert_eq!(parsed.key, b"Key");
        assert_eq!(parsed.value, b"");

        let (parsed, _) = parse_header(b":\n").unwrap();
        assert_eq!(parsed.key, b"");
        assert_eq!(parsed.value, b"");

        let (parsed, _) = parse_header(b"Key:Multi-line\n value").unwrap();
        assert_eq!(parsed.key, b"Key");
        assert_eq!(parsed.value, b"Multi-line\n value");
        assert_eq!(parsed.get_value().unwrap(), "Multi-line value");

        let (parsed, _) = parse_header(b"Key:  Multi\n  line\n value\n").unwrap();
        assert_eq!(parsed.key, b"Key");
        assert_eq!(parsed.value, b"Multi\n  line\n value");
        assert_eq!(parsed.get_value().unwrap(), "Multi line value");

        let (parsed, _) = parse_header(b"Key: One\nKey2: Two").unwrap();
        assert_eq!(parsed.key, b"Key");
        assert_eq!(parsed.value, b"One");

        let (parsed, _) = parse_header(b"Key: One\n\tOverhang").unwrap();
        assert_eq!(parsed.key, b"Key");
        assert_eq!(parsed.value, b"One\n\tOverhang");
        assert_eq!(parsed.get_value().unwrap(), "One Overhang");

        let (parsed, _) = parse_header(b"SPAM: VIAGRA \xAE").unwrap();
        assert_eq!(parsed.key, b"SPAM");
        assert_eq!(parsed.value, b"VIAGRA \xAE");
        assert_eq!(parsed.get_value().unwrap(), "VIAGRA \u{ae}");

        parse_header(b" Leading: Space").unwrap_err();
        parse_header(b"Just a string").unwrap_err();
        parse_header(b"Key\nBroken: Value").unwrap_err();
    }

    #[test]
    fn parse_encoded_headers() {
        let (parsed, _) = parse_header(b"Subject: =?iso-8859-1?Q?=A1Hola,_se=F1or!?=").unwrap();
        assert_eq!(parsed.get_key().unwrap(), "Subject");
        assert_eq!(parsed.get_value().unwrap(), "\u{a1}Hola, se\u{f1}or!");

        let (parsed, _) = parse_header(
            b"Subject: =?iso-8859-1?Q?=A1Hola,?=\n \
                                        =?iso-8859-1?Q?_se=F1or!?=",
        )
        .unwrap();
        assert_eq!(parsed.get_key().unwrap(), "Subject");
        assert_eq!(parsed.get_value().unwrap(), "\u{a1}Hola, se\u{f1}or!");

        let (parsed, _) = parse_header(b"Euro: =?utf-8?Q?=E2=82=AC?=").unwrap();
        assert_eq!(parsed.get_key().unwrap(), "Euro");
        assert_eq!(parsed.get_value().unwrap(), "\u{20ac}");

        let (parsed, _) = parse_header(b"HelloWorld: =?utf-8?B?aGVsbG8gd29ybGQ=?=").unwrap();
        assert_eq!(parsed.get_value().unwrap(), "hello world");

        let (parsed, _) = parse_header(b"Empty: =?utf-8?Q??=").unwrap();
        assert_eq!(parsed.get_value().unwrap(), "");

        let (parsed, _) = parse_header(b"Incomplete: =?").unwrap();
        assert_eq!(parsed.get_value().unwrap(), "=?");

        let (parsed, _) = parse_header(b"BadEncoding: =?garbage?Q??=").unwrap();
        assert_eq!(parsed.get_value().unwrap(), "=?garbage?Q??=");

        let (parsed, _) = parse_header(b"Invalid: =?utf-8?Q?=E2=AC?=").unwrap();
        assert_eq!(parsed.get_value().unwrap(), "\u{fffd}");

        let (parsed, _) = parse_header(b"LineBreak: =?utf-8?Q?=E2=82\n =AC?=").unwrap();
        assert_eq!(parsed.get_value().unwrap(), "=?utf-8?Q?=E2=82 =AC?=");

        let (parsed, _) = parse_header(b"NotSeparateWord: hello=?utf-8?Q?world?=").unwrap();
        assert_eq!(parsed.get_value().unwrap(), "hello=?utf-8?Q?world?=");

        let (parsed, _) = parse_header(b"NotSeparateWord2: =?utf-8?Q?hello?=world").unwrap();
        assert_eq!(parsed.get_value().unwrap(), "=?utf-8?Q?hello?=world");

        let (parsed, _) = parse_header(b"Key: \"=?utf-8?Q?value?=\"").unwrap();
        assert_eq!(parsed.get_value().unwrap(), "\"value\"");

        let (parsed, _) = parse_header(b"Subject: =?utf-8?q?=5BOntario_Builder=5D_Understanding_home_shopping_=E2=80=93_a_q?=\n \
                                        =?utf-8?q?uick_survey?=")
            .unwrap();
        assert_eq!(parsed.get_key().unwrap(), "Subject");
        assert_eq!(
            parsed.get_value().unwrap(),
            "[Ontario Builder] Understanding home shopping \u{2013} a quick survey"
        );

        let (parsed, _) = parse_header(b"Subject: =?ISO-2022-JP?B?GyRCRnwbKEI=?=\n\t=?ISO-2022-JP?B?GyRCS1wbKEI=?=\n\t=?ISO-2022-JP?B?GyRCOGwbKEI=?=")
            .unwrap();
        assert_eq!(parsed.get_key().unwrap(), "Subject");
        assert_eq!(parsed.get_value().unwrap(), "\u{65E5}\u{672C}\u{8A9E}");

        let (parsed, _) = parse_header(b"Subject: =?ISO-2022-JP?Q?=1B\x24\x42\x46\x7C=1B\x28\x42?=\n\t=?ISO-2022-JP?Q?=1B\x24\x42\x4B\x5C=1B\x28\x42?=\n\t=?ISO-2022-JP?Q?=1B\x24\x42\x38\x6C=1B\x28\x42?=")
            .unwrap();
        assert_eq!(parsed.get_key().unwrap(), "Subject");
        assert_eq!(parsed.get_value().unwrap(), "\u{65E5}\u{672C}\u{8A9E}");

        let (parsed, _) = parse_header(b"Subject: =?UTF-7?Q?+JgM-?=").unwrap();
        assert_eq!(parsed.get_key().unwrap(), "Subject");
        assert_eq!(parsed.get_value().unwrap(), "\u{2603}");

        let (parsed, _) =
            parse_header(b"Content-Type: image/jpeg; name=\"=?UTF-8?B?MDY2MTM5ODEuanBn?=\"")
                .unwrap();
        assert_eq!(parsed.get_key().unwrap(), "Content-Type");
        assert_eq!(
            parsed.get_value().unwrap(),
            "image/jpeg; name=\"06613981.jpg\""
        );

        let (parsed, _) = parse_header(
            b"From: =?UTF-8?Q?\"Motorola_Owners=E2=80=99_Forums\"_?=<forums@motorola.com>",
        )
        .unwrap();
        assert_eq!(parsed.get_key().unwrap(), "From");
        assert_eq!(
            parsed.get_value().unwrap(),
            "\"Motorola Owners\u{2019} Forums\" <forums@motorola.com>"
        );
    }

    #[test]
    fn parse_multiple_headers() {
        let (parsed, _) = parse_headers(b"Key: Value\nTwo: Second").unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].key, b"Key");
        assert_eq!(parsed[0].value, b"Value");
        assert_eq!(parsed[1].key, b"Two");
        assert_eq!(parsed[1].value, b"Second");

        let (parsed, _) =
            parse_headers(b"Key: Value\n Overhang\nTwo: Second\nThree: Third").unwrap();
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].key, b"Key");
        assert_eq!(parsed[0].value, b"Value\n Overhang");
        assert_eq!(parsed[1].key, b"Two");
        assert_eq!(parsed[1].value, b"Second");
        assert_eq!(parsed[2].key, b"Three");
        assert_eq!(parsed[2].value, b"Third");

        let (parsed, _) = parse_headers(b"Key: Value\nTwo: Second\n\nBody").unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].key, b"Key");
        assert_eq!(parsed[0].value, b"Value");
        assert_eq!(parsed[1].key, b"Two");
        assert_eq!(parsed[1].value, b"Second");

        let (parsed, _) = parse_headers(
            concat!(
                "Return-Path: <kats@foobar.staktrace.com>\n",
                "X-Original-To: kats@baz.staktrace.com\n",
                "Delivered-To: kats@baz.staktrace.com\n",
                "Received: from foobar.staktrace.com (localhost [127.0.0.1])\n",
                "    by foobar.staktrace.com (Postfix) with ESMTP id \
                 139F711C1C34\n",
                "    for <kats@baz.staktrace.com>; Fri, 27 May 2016 02:34:26 \
                 -0400 (EDT)\n",
                "Date: Fri, 27 May 2016 02:34:25 -0400\n",
                "To: kats@baz.staktrace.com\n",
                "From: kats@foobar.staktrace.com\n",
                "Subject: test Fri, 27 May 2016 02:34:25 -0400\n",
                "X-Mailer: swaks v20130209.0 jetmore.org/john/code/swaks/\n",
                "Message-Id: \
                 <20160527063426.139F711C1C34@foobar.staktrace.com>\n",
                "\n",
                "This is a test mailing\n"
            )
            .as_bytes(),
        )
        .unwrap();
        assert_eq!(parsed.len(), 10);
        assert_eq!(parsed[0].key, b"Return-Path");
        assert_eq!(parsed[9].key, b"Message-Id");

        let (parsed, _) =
            parse_headers(b"Key: Value\nAnotherKey: AnotherValue\nKey: Value2\nKey: Value3\n")
                .unwrap();
        assert_eq!(parsed.len(), 4);
        assert_eq!(
            parsed.get_first_value("Key").unwrap(),
            Some("Value".to_string())
        );
        assert_eq!(
            parsed.get_all_values("Key").unwrap(),
            vec!["Value", "Value2", "Value3"]
        );
        assert_eq!(
            parsed.get_first_value("AnotherKey").unwrap(),
            Some("AnotherValue".to_string())
        );
        assert_eq!(
            parsed.get_all_values("AnotherKey").unwrap(),
            vec!["AnotherValue"]
        );
        assert_eq!(parsed.get_first_value("NoKey").unwrap(), None);
        assert_eq!(
            parsed.get_all_values("NoKey").unwrap(),
            Vec::<String>::new()
        );

        let (parsed, _) = parse_headers(b"Key: value\r\nWith: CRLF\r\n\r\nBody").unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(
            parsed.get_first_value("Key").unwrap(),
            Some("value".to_string())
        );
        assert_eq!(
            parsed.get_first_value("With").unwrap(),
            Some("CRLF".to_string())
        );

        assert_match!(
            parse_headers(b"Bad\nKey").unwrap_err(),
            MailParseError::Generic(_)
        );
        assert_match!(
            parse_headers(b"K:V\nBad\nKey").unwrap_err(),
            MailParseError::Generic(_)
        );
    }

    #[test]
    fn test_parse_content_type() {
        let ctype = parse_content_type("text/html; charset=utf-8");
        assert_eq!(ctype.mimetype, "text/html");
        assert_eq!(ctype.charset, "utf-8");
        assert_eq!(ctype.params.get("boundary"), None);

        let ctype = parse_content_type(" foo/bar; x=y; charset=\"fake\" ; x2=y2");
        assert_eq!(ctype.mimetype, "foo/bar");
        assert_eq!(ctype.charset, "fake");
        assert_eq!(ctype.params.get("boundary"), None);

        let ctype = parse_content_type(" multipart/bar; boundary=foo ");
        assert_eq!(ctype.mimetype, "multipart/bar");
        assert_eq!(ctype.charset, "us-ascii");
        assert_eq!(ctype.params.get("boundary").unwrap(), "foo");
    }

    #[test]
    fn test_parse_content_disposition() {
        let dis = parse_content_disposition("inline");
        assert_eq!(dis.disposition, DispositionType::Inline);
        assert_eq!(dis.params.get("name"), None);
        assert_eq!(dis.params.get("filename"), None);

        let dis = parse_content_disposition(
            " attachment; x=y; charset=\"fake\" ; x2=y2; name=\"King Joffrey.death\"",
        );
        assert_eq!(dis.disposition, DispositionType::Attachment);
        assert_eq!(
            dis.params.get("name"),
            Some(&"King Joffrey.death".to_string())
        );
        assert_eq!(dis.params.get("filename"), None);

        let dis = parse_content_disposition(" form-data");
        assert_eq!(dis.disposition, DispositionType::FormData);
        assert_eq!(dis.params.get("name"), None);
        assert_eq!(dis.params.get("filename"), None);
    }

    #[test]
    fn test_parse_mail() {
        let mail = parse_mail(b"Key: value\r\n\r\nSome body stuffs").unwrap();
        assert_eq!(mail.headers.len(), 1);
        assert_eq!(mail.headers[0].get_key().unwrap(), "Key");
        assert_eq!(mail.headers[0].get_value().unwrap(), "value");
        assert_eq!(mail.ctype.mimetype, "text/plain");
        assert_eq!(mail.ctype.charset, "us-ascii");
        assert_eq!(mail.ctype.params.get("boundary"), None);
        assert_eq!(mail.body, b"Some body stuffs");
        assert_eq!(mail.get_body_raw().unwrap(), b"Some body stuffs");
        assert_eq!(mail.get_body().unwrap(), "Some body stuffs");
        assert_eq!(mail.subparts.len(), 0);

        let mail = parse_mail(
            concat!(
                "Content-Type: MULTIpart/alternative; bounDAry=myboundary\r\n\r\n",
                "--myboundary\r\n",
                "Content-Type: text/plain\r\n\r\n",
                "This is the plaintext version.\r\n",
                "--myboundary\r\n",
                "Content-Type: text/html;chARset=utf-8\r\n\r\n",
                "This is the <b>HTML</b> version with fake --MYBOUNDARY.\r\n",
                "--myboundary--"
            )
            .as_bytes(),
        )
        .unwrap();
        assert_eq!(mail.headers.len(), 1);
        assert_eq!(mail.headers[0].get_key().unwrap(), "Content-Type");
        assert_eq!(mail.ctype.mimetype, "multipart/alternative");
        assert_eq!(mail.ctype.charset, "us-ascii");
        assert_eq!(mail.ctype.params.get("boundary").unwrap(), "myboundary");
        assert_eq!(mail.subparts.len(), 2);
        assert_eq!(mail.subparts[0].headers.len(), 1);
        assert_eq!(mail.subparts[0].ctype.mimetype, "text/plain");
        assert_eq!(mail.subparts[0].ctype.charset, "us-ascii");
        assert_eq!(mail.subparts[0].ctype.params.get("boundary"), None);
        assert_eq!(mail.subparts[1].ctype.mimetype, "text/html");
        assert_eq!(mail.subparts[1].ctype.charset, "utf-8");
        assert_eq!(mail.subparts[1].ctype.params.get("boundary"), None);

        let mail =
            parse_mail(b"Content-Transfer-Encoding: base64\r\n\r\naGVsbG 8gd\r\n29ybGQ=").unwrap();
        assert_eq!(mail.get_body_raw().unwrap(), b"hello world");
        assert_eq!(mail.get_body().unwrap(), "hello world");

        let mail =
            parse_mail(b"Content-Type: text/plain; charset=x-unknown\r\n\r\nhello world").unwrap();
        assert_eq!(mail.get_body_raw().unwrap(), b"hello world");
        assert_eq!(mail.get_body().unwrap(), "hello world");

        let mail = parse_mail(b"ConTENT-tyPE: text/html\r\n\r\nhello world").unwrap();
        assert_eq!(mail.ctype.mimetype, "text/html");
        assert_eq!(mail.get_body_raw().unwrap(), b"hello world");
        assert_eq!(mail.get_body().unwrap(), "hello world");

        let mail = parse_mail(
            b"Content-Type: text/plain; charset=UTF-7\r\nContent-Transfer-Encoding: quoted-printable\r\n\r\n+JgM-",
        ).unwrap();
        assert_eq!(mail.get_body_raw().unwrap(), b"+JgM-");
        assert_eq!(mail.get_body().unwrap(), "\u{2603}");

        let mail = parse_mail(b"Content-Type: text/plain; charset=UTF-7\r\n\r\n+JgM-").unwrap();
        assert_eq!(mail.get_body_raw().unwrap(), b"+JgM-");
        assert_eq!(mail.get_body().unwrap(), "\u{2603}");
    }

    #[test]
    fn test_missing_terminating_boundary() {
        let mail = parse_mail(
            concat!(
                "Content-Type: multipart/alternative; boundary=myboundary\r\n\r\n",
                "--myboundary\r\n",
                "Content-Type: text/plain\r\n\r\n",
                "part0\r\n",
                "--myboundary\r\n",
                "Content-Type: text/html\r\n\r\n",
                "part1\r\n"
            )
            .as_bytes(),
        )
        .unwrap();
        assert_eq!(mail.subparts[0].get_body().unwrap(), "part0\r\n");
        assert_eq!(mail.subparts[1].get_body().unwrap(), "part1\r\n");
    }

    #[test]
    fn test_missing_body() {
        let parsed =
            parse_mail("Content-Type: multipart/related; boundary=\"----=_\"\n".as_bytes())
                .unwrap();
        assert_eq!(parsed.headers[0].get_key().unwrap(), "Content-Type");
        assert_eq!(parsed.get_body_raw().unwrap(), b"");
        assert_eq!(parsed.get_body().unwrap(), "");
    }

    #[test]
    fn test_no_headers_in_subpart() {
        let mail = parse_mail(
            concat!(
                "Content-Type: multipart/report; report-type=delivery-status;\n",
                "\tboundary=\"1404630116.22555.postech.q0.x.x.x\"\n",
                "\n",
                "--1404630116.22555.postech.q0.x.x.x\n",
                "\n",
                "--1404630116.22555.postech.q0.x.x.x--\n"
            )
            .as_bytes(),
        )
        .unwrap();
        assert_eq!(mail.ctype.mimetype, "multipart/report");
        assert_eq!(mail.subparts[0].headers.len(), 0);
        assert_eq!(mail.subparts[0].ctype.mimetype, "text/plain");
        assert_eq!(mail.subparts[0].get_body_raw().unwrap(), b"");
        assert_eq!(mail.subparts[0].get_body().unwrap(), "");
    }

    #[test]
    fn test_empty() {
        let mail = parse_mail("".as_bytes()).unwrap();
        assert_eq!(mail.get_body_raw().unwrap(), b"");
        assert_eq!(mail.get_body().unwrap(), "");
    }

    #[test]
    fn test_is_boundary_multibyte() {
        // Bug #26, Incorrect unwrap() guard in is_boundary()
        // 6x'REPLACEMENT CHARACTER', but 18 bytes of data:
        let test = "\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}";
        assert!(is_boundary(test, Some(8)));
    }

    #[test]
    fn test_dont_panic_for_value_with_new_lines() {
        let parsed = parse_param_content(r#"Content-Type: application/octet-stream; name=""#);
        assert_eq!(parsed.params["name"], "\"");
    }

    #[test]
    fn test_default_content_encoding() {
        let mail = parse_mail(b"Content-Type: text/plain; charset=UTF-7\r\n\r\n+JgM-").unwrap();
        let body = mail.get_body_encoded().unwrap();
        match body {
            Body::SevenBit(body) => {
                assert_eq!(body.get_raw(), b"+JgM-");
                assert_eq!(body.get_as_string().unwrap(), "\u{2603}");
            }
            _ => assert!(false),
        };
    }

    #[test]
    fn test_7bit_content_encoding() {
        let mail = parse_mail(b"Content-Type: text/plain; charset=UTF-7\r\nContent-Transfer-Encoding: 7bit\r\n\r\n+JgM-").unwrap();
        let body = mail.get_body_encoded().unwrap();
        match body {
            Body::SevenBit(body) => {
                assert_eq!(body.get_raw(), b"+JgM-");
                assert_eq!(body.get_as_string().unwrap(), "\u{2603}");
            }
            _ => assert!(false),
        };
    }

    #[test]
    fn test_8bit_content_encoding() {
        let mail = parse_mail(b"Content-Type: text/plain; charset=UTF-7\r\nContent-Transfer-Encoding: 8bit\r\n\r\n+JgM-").unwrap();
        let body = mail.get_body_encoded().unwrap();
        match body {
            Body::EightBit(body) => {
                assert_eq!(body.get_raw(), b"+JgM-");
                assert_eq!(body.get_as_string().unwrap(), "\u{2603}");
            }
            _ => assert!(false),
        };
    }

    #[test]
    fn test_quoted_printable_content_encoding() {
        let mail = parse_mail(
            b"Content-Type: text/plain; charset=UTF-7\r\nContent-Transfer-Encoding: quoted-printable\r\n\r\n+JgM-",
        ).unwrap();
        match mail.get_body_encoded().unwrap() {
            Body::QuotedPrintable(body) => {
                assert_eq!(body.get_raw(), b"+JgM-");
                assert_eq!(body.get_decoded().unwrap(), b"+JgM-");
                assert_eq!(body.get_decoded_as_string().unwrap(), "\u{2603}");
            }
            _ => assert!(false),
        };
    }

    #[test]
    fn test_base64_content_encoding() {
        let mail =
            parse_mail(b"Content-Transfer-Encoding: base64\r\n\r\naGVsbG 8gd\r\n29ybGQ=").unwrap();
        match mail.get_body_encoded().unwrap() {
            Body::Base64(body) => {
                assert_eq!(body.get_raw(), b"aGVsbG 8gd\r\n29ybGQ=");
                assert_eq!(body.get_decoded().unwrap(), b"hello world");
                assert_eq!(body.get_decoded_as_string().unwrap(), "hello world");
            }
            _ => assert!(false),
        };
    }

    #[test]
    fn test_binary_content_encoding() {
        let mail = parse_mail(b"Content-Transfer-Encoding: binary\r\n\r\n######").unwrap();
        let body = mail.get_body_encoded().unwrap();
        match body {
            Body::Binary(body) => {
                assert_eq!(body.get_raw(), b"######");
            }
            _ => assert!(false),
        };
    }

    #[test]
    fn test_fuzzer_testcase() {
        const INPUT: &'static str = "U3ViamVjdDplcy1UeXBlOiBtdW50ZW50LVV5cGU6IW11bAAAAAAAAAAAamVjdDplcy1UeXBlOiBtdW50ZW50LVV5cGU6IG11bAAAAAAAAAAAAAAAAABTTUFZdWJqZf86OiP/dCBTdWJqZWN0Ol8KRGF0ZTog/////////////////////wAAAAAAAAAAAHQgYnJmAHQgYnJmZXItRW5jeXBlOnY9NmU3OjA2OgAAAAAAAAAAAAAAADEAAAAAAP/8mAAAAAAAAAAA+f///wAAAAAAAP8AAAAAAAAAAAAAAAAAAAAAAAAAPT0/PzEAAAEAAA==";

        if let Ok(parsed) = parse_mail(&base64::decode(INPUT).unwrap()) {
            if let Ok(Some(date)) = parsed.headers.get_first_value("Date") {
                let _ = dateparse(&date);
            }
        }
    }

    #[test]
    fn test_fuzzer_testcase_2() {
        const INPUT: &'static str = "U3ViamVjdDogVGhpcyBpcyBhIHRlc3QgZW1haWwKQ29udGVudC1UeXBlOiBtdWx0aXBhcnQvYWx0ZXJuYXRpdmU7IGJvdW5kYXJ5PczMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMZm9vYmFyCkRhdGU6IFN1biwgMDIgT2MKCi1TdWJqZWMtZm9vYmFydDo=";
        if let Ok(parsed) = parse_mail(&base64::decode(INPUT).unwrap()) {
            if let Ok(Some(date)) = parsed.headers.get_first_value("Date") {
                let _ = dateparse(&date);
            }
        }
    }
}

