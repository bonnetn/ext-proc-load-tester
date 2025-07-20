#![allow(warnings)]

pub(crate) mod envoy {
    pub(crate) mod service {
        pub(crate) mod ext_proc {
            pub(crate) mod v3 {
                tonic::include_proto!("envoy.service.ext_proc.v3");
            }
        }
    }

    pub(crate) mod config {
        pub(crate) mod core {
            pub(crate) mod v3 {
                tonic::include_proto!("envoy.config.core.v3");
            }
        }
    }

    pub(crate) mod extensions {
        pub(crate) mod filters {
            pub(crate) mod http {
                pub(crate) mod ext_proc {
                    pub(crate) mod v3 {
                        tonic::include_proto!("envoy.extensions.filters.http.ext_proc.v3");
                    }
                }
            }
        }
    }

    pub(crate) mod r#type {
        pub(crate) mod v3 {
            tonic::include_proto!("envoy.r#type.v3");
        }
    }
}

pub(crate) mod xds {
    pub(crate) mod core {
        pub(crate) mod v3 {
            tonic::include_proto!("xds.core.v3");
        }
    }
}
