use std::{
    collections::HashMap,
    sync::Arc,
    path::Path
};

use serde::{
    Serialize, Deserialize,
    de::{
        Deserializer,
    },
};
use failure::Error;
use futures::{Future, future::{self, Either}};
use vec1::Vec1;

use mail_core::{Resource, Source, IRI, Context};

use super::{
    Template,
    TemplateEngine,
    CwdBaseDir,
    PathRebaseable,
    InnerTemplate,
    Subject,
    UnsupportedPathError,
};

/// Type used when deserializing a template using serde.
///
/// This type should only be used as intermediate type
/// used for deserialization as templates need to be
/// bundled with a template engine.
///
/// # Serialize/Deserialize
///
/// The derserialization currently only works with
/// self-describing data formats.
///
/// There are a number of shortcuts to deserialize
/// resources (from emebddings and/or attachments):
///
/// - Resources can be deserialized normally (from a externally tagged enum)
/// - Resources can be deserialized from the serialized repr of `Source`
/// - Resources can be deserialized from a string which is used to create
///   a `Resource::Source` with a iri using the `path` scheme and the string
///   content as the iris "tail".
#[derive(Debug, Serialize, Deserialize)]
pub struct TemplateBase<TE: TemplateEngine> {
    #[serde(rename="name")]
    template_name: String,
    #[serde(default)]
    base_dir: Option<CwdBaseDir>,
    subject: LazySubject,
    bodies: Vec1<TE::LazyBodyTemplate>,
    //TODO impl. deserialize where
    // resource:String -> IRI::new("path", resource) -> Resource::Source
    #[serde(deserialize_with="deserialize_embeddings")]
    embeddings: HashMap<String, Resource>,
    #[serde(deserialize_with="deserialize_attachments")]
    attachments: Vec<Resource>,
}

impl<TE> TemplateBase<TE>
    where TE: TemplateEngine
{

    //TODO!! make this load all embeddings/attachments and make it a future
    /// Couples the template base with a specific engine instance.
    pub fn load(self, mut engine: TE, default_base_dir: CwdBaseDir, ctx: &impl Context) -> impl Future<Item=Template<TE>, Error=Error> {
        let TemplateBase {
            template_name,
            base_dir,
            subject,
            bodies,
            mut embeddings,
            mut attachments
        } = self;

        let base_dir = base_dir.unwrap_or(default_base_dir);

        //FIXME[rust/catch block] use catch block
        let catch_res = (|| -> Result<_, Error> {
            let subject = Subject{ template_id: engine.load_subject_template(subject.template_string)? };

            let bodies = bodies.try_mapped(|mut lazy_body| -> Result<_, Error> {
                lazy_body.rebase_to_include_base_dir(&base_dir)?;
                Ok(engine.load_body_template(lazy_body)?)
            })?;

            for embedding in embeddings.values_mut() {
                embedding.rebase_to_include_base_dir(&base_dir)?;
            }

            for attachment in attachments.iter_mut() {
                attachment.rebase_to_include_base_dir(&base_dir)?;
            }

            Ok((subject, bodies))
        })();

        let (subject, bodies) =
            match catch_res {
                Ok(vals) => vals,
                Err(err) => { return Either::B(future::err(err)); }
            };

        let loading_fut = Resource::load_container(embeddings, ctx)
            .join(Resource::load_container(attachments, ctx));

        let fut = loading_fut
            .map_err(Error::from)
            .map(|(embeddings, attachments)| {
                let inner = InnerTemplate {
                    template_name,
                    base_dir,
                    subject,
                    bodies,
                    embeddings,
                    attachments,
                    engine
                };

                Template { inner: Arc::new(inner) }
            });

        Either::A(fut)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct LazySubject {
    #[serde(flatten)]
    template_string: String
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ResourceDeserializationHelper {
    // This allows specifying resources in three ways.
    // 1. as tagged enum `Resource` (e.g. `{"Source": { "iri": ...}}}`)
    // 2. as struct `Source` (e.g. `{"iri": ...}` )
    // 3. as String which is interpreted as path iri
    Normal(Resource),
    FromSource(Source),
    FromString(String)
}

impl Into<Resource> for ResourceDeserializationHelper {
    fn into(self) -> Resource {
        use self::ResourceDeserializationHelper::*;
        match self {
            Normal(resource) => resource,
            FromString(string) => {
                let source = Source {
                    //UNWRAP_SAFE: only scheme validation could fail,
                    // but its static "path" which is known to be valid
                    iri: IRI::from_parts("path", &string).unwrap(),
                    use_media_type: Default::default(),
                    use_file_name: Default::default()
                };

                Resource::Source(source)
            },
            FromSource(source) => Resource::Source(source)
        }
    }
}

pub fn deserialize_embeddings<'de, D>(deserializer: D)
    -> Result<HashMap<String, Resource>, D::Error>
    where D: Deserializer<'de>
{
    //FIXME[perf] write custom visitor etc.
    let map = <HashMap<String, ResourceDeserializationHelper>>
        ::deserialize(deserializer)?;

    let map = map.into_iter()
        .map(|(k, helper)| (k, helper.into()))
        .collect();

    Ok(map)
}

pub fn deserialize_attachments<'de, D>(deserializer: D)
    -> Result<Vec<Resource>, D::Error>
    where D: Deserializer<'de>
{
    //FIXME[perf] write custom visitor etc.
    let vec = <Vec<ResourceDeserializationHelper>>
        ::deserialize(deserializer)?;

    let vec = vec.into_iter()
        .map(|helper| helper.into())
        .collect();

    Ok(vec)
}

//TODO make base dir default to the dir the template file is in if it's parsed from a template file.

/// Common implementation for a type for [`TemplateEngine::LazyBodyTemplate`].
///
/// This impl. gives bodies a field `embeddings` which is a mapping of embedding
/// names to embeddings (using `deserialize_embeddings`) a `iri` field which
/// allows specifying the template (e.g. `"path:body.html"`) and can be relative
/// to the base dir. It also allows just specifying a string as template which defaults
/// to using `iri = "path:<thestring>"` and no embeddings.
#[derive(Debug, Serialize)]
pub struct StandardLazyBodyTemplate {
    pub iri: IRI,
    pub embeddings: HashMap<String, Resource>
}


impl PathRebaseable for StandardLazyBodyTemplate {
    fn rebase_to_include_base_dir(&mut self, base_dir: impl AsRef<Path>)
        -> Result<(), UnsupportedPathError>
    {
        self.iri.rebase_to_include_base_dir(base_dir)
    }

    fn rebase_to_exclude_base_dir(&mut self, base_dir: impl AsRef<Path>)
        -> Result<(), UnsupportedPathError>
    {
        self.iri.rebase_to_exclude_base_dir(base_dir)
    }
}


#[derive(Deserialize)]
#[serde(untagged)]
enum StandardLazyBodyTemplateDeserializationHelper {
    ShortForm(String),
    LongForm {
        iri: IRI,
        #[serde(default)]
        #[serde(deserialize_with="deserialize_embeddings")]
        embeddings: HashMap<String, Resource>
    }
}

impl<'de> Deserialize<'de> for StandardLazyBodyTemplate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        use self::StandardLazyBodyTemplateDeserializationHelper::*;
        let helper = StandardLazyBodyTemplateDeserializationHelper::deserialize(deserializer)?;
        let ok_val =
            match helper {
                ShortForm(string) => {
                    //UNWRAP_SAFE: only scheme can fail but is known to be ok
                    let iri = IRI::from_parts("path", &string).unwrap();
                    StandardLazyBodyTemplate {
                        iri,
                        embeddings: Default::default()
                    }
                },
                LongForm {iri, embeddings} => StandardLazyBodyTemplate { iri, embeddings }
            };
        Ok(ok_val)
    }
}


#[cfg(test)]
mod test {
    use toml;
    use super::*;

    fn test_source_iri(resource: &Resource, iri: &str) {
        if let &Resource::Source(ref source) = resource {
            assert_eq!(source.iri.as_str(), iri);
        } else {
            panic!("unexpected resource expected resource with source and iri {:?} but got {:?}", iri, resource);
        }
    }

    mod attachment_deserialization {
        use super::*;
        use super::super::deserialize_attachments;

        #[derive(Serialize, Deserialize)]
        struct Wrapper {
            #[serde(deserialize_with="deserialize_attachments")]
            attachments: Vec<Resource>
        }


        #[test]
        fn should_deserialize_from_strings() {
            let raw_toml = r#"
                attachments = ["notes.md", "pic.xd"]
            "#;

            let Wrapper { attachments } = toml::from_str(raw_toml).unwrap();

            assert_eq!(attachments.len(), 2);
            test_source_iri(&attachments[0], "path:notes.md");
            test_source_iri(&attachments[1], "path:pic.xd");
        }

        #[test]
        fn should_deserialize_from_sources() {
            let raw_toml = r#"
                [[attachments]]
                Source = {iri="https://fun.example"}
                [[attachments]]
                iri="path:pic.xd"
            "#;

            let Wrapper { attachments } = toml::from_str(raw_toml).unwrap();

            assert_eq!(attachments.len(), 2);
            test_source_iri(&attachments[0], "https://fun.example");
            test_source_iri(&attachments[1], "path:pic.xd");
        }

        #[test]
        fn check_if_data_is_deserializable_like_expected() {
            use mail_core::Data;

            let raw_toml = r#"
                media_type = "text/plain; charset=utf-8"
                buffer = [65,65,65,66,65]
                content_id = "c0rc3rcr0q0v32@example.example"
            "#;

            let data: Data = toml::from_str(raw_toml).unwrap();

            assert_eq!(data.content_id().as_str(), "c0rc3rcr0q0v32@example.example");
            assert_eq!(&**data.buffer(), b"AAABA" as &[u8]);
        }

        #[test]
        fn should_deserialize_from_data() {
            let raw_toml = r#"
                [[attachments]]
                [attachments.Data]
                media_type = "text/plain; charset=utf-8"
                buffer = [65,65,65,66,65]
                content_id = "c0rc3rcr0q0v32@example.example"
            "#;

            let Wrapper { attachments } = toml::from_str(raw_toml).unwrap();

            assert_eq!(attachments.len(), 1);
        }
    }

    mod embedding_deserialization {
        use super::*;
        use super::super::deserialize_embeddings;

        #[derive(Serialize, Deserialize)]
        struct Wrapper {
            #[serde(deserialize_with="deserialize_embeddings")]
            embeddings: HashMap<String, Resource>
        }

        #[test]
        fn should_deserialize_with_short_forms() {
            let raw_toml = r#"
                [embeddings]
                pic = "hy-ya"
                pic2 = { iri = "path:ay-ya" }
                [embeddings.pic3.Data]
                media_type = "text/plain; charset=utf-8"
                buffer = [65,65,65,66,65]
                content_id = "c0rc3rcr0q0v32@example.example"
                [embeddings.pic4.Source]
                iri = "path:nay-nay-way"
            "#;

            let Wrapper { embeddings } = toml::from_str(raw_toml).unwrap();

            assert_eq!(embeddings.len(), 4);
            assert!(embeddings.contains_key("pic"));
            assert!(embeddings.contains_key("pic2"));
            assert!(embeddings.contains_key("pic3"));
            assert!(embeddings.contains_key("pic4"));
            test_source_iri(&embeddings["pic"], "path:hy-ya");
            test_source_iri(&embeddings["pic2"], "path:ay-ya");
            test_source_iri(&embeddings["pic4"], "path:nay-nay-way");
            assert_eq!(embeddings["pic3"].content_id().unwrap().as_str(), "c0rc3rcr0q0v32@example.example");
        }
    }

    #[allow(non_snake_case)]
    mod StandardLazyBodyTemplate {
        use super::*;
        use super::super::StandardLazyBodyTemplate;

        #[derive(Serialize, Deserialize)]
        struct Wrapper {
            body: StandardLazyBodyTemplate
        }

        #[test]
        fn should_deserialize_from_string() {
            let toml_str = r#"
                body = "template.html.hbs"
            "#;

            let Wrapper { body } = toml::from_str(toml_str).unwrap();
            assert_eq!(body.iri.as_str(), "path:template.html.hbs");
            assert_eq!(body.embeddings.len(), 0);
        }

        #[test]
        fn should_deserialize_from_object_without_embeddings() {
            let toml_str = r#"
                body = { iri="path:t.d" }
            "#;

            let Wrapper { body }= toml::from_str(toml_str).unwrap();
            assert_eq!(body.iri.as_str(), "path:t.d");
            assert_eq!(body.embeddings.len(), 0);
        }

        #[test]
        fn should_deserialize_from_object_with_empty_embeddings() {
            let toml_str = r#"
                body = { iri="path:t.d", embeddings={} }
            "#;

            let Wrapper { body } = toml::from_str(toml_str).unwrap();
            assert_eq!(body.iri.as_str(), "path:t.d");
            assert_eq!(body.embeddings.len(), 0);
        }

        #[test]
        fn should_deserialize_from_object_with_short_from_embeddings() {
            let toml_str = r#"
                body = { iri="path:t.d", embeddings={ pic1="the_embeddings" } }
            "#;

            let Wrapper { body } = toml::from_str(toml_str).unwrap();
            assert_eq!(body.iri.as_str(), "path:t.d");
            assert_eq!(body.embeddings.len(), 1);

            let (key, resource) = body.embeddings.iter().next().unwrap();
            assert_eq!(key, "pic1");

            test_source_iri(resource, "path:the_embeddings");
        }
    }
}