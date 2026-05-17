macro_rules! set_child_prop {
    ( $p:tt . $name:ident = $item:expr) => {
        match $p {
            crate::protocol::property::Property::Tree{ children, ..} => {
                let found = children.iter_mut().find(|p| match p {
                    crate::protocol::property::Property::Tree { name, .. } => name == stringify!($name),
                    crate::protocol::property::Property::Node(crate::protocol::property::Item { name, .. }) => name == stringify!($name),
                });
                if let Some(crate::protocol::property::Property::Node(crate::protocol::property::Item { value, .. })) = found {
                    *value = $item.value;
                } else {
                    let pp = crate::protocol::property::Property::Node(crate::protocol::property::Item {
                        name: $item.name.to_string(),
                        type_: $item.type_,
                        value: $item.value,
                        metadata: crate::protocol::property::Metadata::Undefined,
                        phantom: core::marker::PhantomData,
                    });
                    children.push(pp);
                }

            }
            _ => unreachable!(),
        }
    };
    ({ $p:expr } . $name:ident $($rest:tt)*) => {
        set_child_prop!(
            {{
                match $p.find_mut(stringify!($name)) {
                    Some(property) => property,
                    None => {
                        let pp = crate::protocol::property::Property::new_tree(stringify!($name));
                        $p.push(pp);
                        $p.find_mut(stringify!($name)).unwrap()
                    }
                }
            }} $($rest)*
        )
    };
}

macro_rules! get_child_prop {
    ({ $popt:expr }) => {
        match $popt {
            Some(crate::protocol::property::Property::Node( item )) => {
                crate::protocol::property::Item {
                    name: item.name.clone(),
                    type_: item.type_,
                    value: item.value.clone(),
                    metadata: item.metadata.clone(),
                    phantom: core::marker::PhantomData,
                }
            },
            _ => crate::protocol::property::Item {
                name: String::new(),
                type_: crate::protocol::property::PropertyType::ReadOnly,
                value: crate::protocol::property::PropValue::Null,
                metadata: crate::protocol::property::Metadata::Undefined,
                phantom: core::marker::PhantomData,
            }
        }
    };
    ({ $popt:expr } .to_string()) => {{
        if let Some(crate::protocol::property::Property::Node( item )) = $popt {
            item.get_string()
        } else {
            None
        }
    }};
    ({ $popt:expr } . $name:ident $($rest:tt)*) => {
        get_child_prop!(
            { $popt.and_then(|p| p.find(stringify!($name))) } $($rest)*
        )
    };
}

macro_rules! get_prop {
    ($root:tt . $path:literal $($rest:tt)*) => {
        get_child_prop!(
            { $root.responses.iter().find(|&r| r.from == $path).and_then(|r| r.content.as_ref()) } $($rest)*
        )
    };
}

#[cfg(test)]
mod tests {
    use crate::protocol::property::{Item, PropValue};
    use crate::protocol::request::{DaikinRequest, Request};
    use crate::protocol::response::DaikinResponse;

    #[test]
    fn get_prop() {
        let res: DaikinResponse = serde_json::from_str(include_str!("../fixtures/status.json"))
            .expect("Invalid JSON file.");

        let p: Item = get_prop!(res."/dsiot/edge/adr_0100.dgc_status".e_1002.e_A001.p_03);
        assert_eq!(
            format!("{p:?}"),
            r#"Item { name: "p_03", value: 5.6, metadata: Binary(Step(BinaryStep { range: 0.0..=25.5, step: 0.1 })) }"#
        );

        let p: Item = get_prop!(res."/hoge".fuga.piyo);
        assert_eq!(
            format!("{p:?}"),
            r#"Item { name: "", value: Null, metadata: Undefined }"#
        );
    }

    #[test]
    fn set_child_prop() {
        let item: Item<f32> = Item {
            name: "p_03".into(),
            type_: crate::protocol::property::PropertyType::ReadWrite,
            value: PropValue::String("3800".into()),
            metadata: crate::protocol::property::Metadata::Undefined,
            phantom: core::marker::PhantomData,
        };
        let mut prop = crate::protocol::property::Property::new_tree("dgc_status");

        set_child_prop!({ prop }.e_1002.e_A001.p_03 = item);

        let req = DaikinRequest {
            requests: vec![Request {
                op: 3,
                pc: prop,
                to: "/dsiot/edge/adr_0100.dgc_status".into(),
            }],
        };

        assert_eq!(
            serde_json::to_string(&req).unwrap(),
            r#"{"requests":[{"op":3,"pc":{"pn":"dgc_status","pch":[{"pn":"e_1002","pch":[{"pn":"e_A001","pch":[{"pn":"p_03","pv":"3800"}]}]}]},"to":"/dsiot/edge/adr_0100.dgc_status"}]}"#
        );
    }
}
