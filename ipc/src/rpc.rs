// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `rpc!`: one declaration per remote call gives the request enum, the
//! service trait the server implements, the dispatcher that runs a
//! request against it, and the client trait whose methods send the
//! request and decode the answer.
//!
//! ```ignore
//! ipc::rpc! {
//!     request Request, service Service, client Client, dispatch dispatch;
//!     /// Echo a string.
//!     0 => fn echo / Echo (text: String) -> String;
//! }
//! ```

/// See the module documentation.
#[macro_export]
macro_rules! rpc {
    (
        request $req:ident, service $svc:ident, client $cli:ident, dispatch $disp:ident;
        $(
            $(#[$m:meta])*
            $tag:literal => fn $f:ident / $v:ident ( $($a:ident : $t:ty),* $(,)? ) -> $r:ty;
        )*
    ) => {
        /// One request; the variants mirror the service's methods.
        #[derive(Debug, Clone)]
        #[allow(missing_docs)]
        pub enum $req {
            $( $(#[$m])* $v { $($a: $t),* }, )*
        }

        impl $crate::Wire for $req {
            fn put(&self, out: &mut Vec<u8>) {
                match self {
                    $( $req::$v { $($a),* } => {
                        out.push($tag);
                        $( $crate::Wire::put($a, out); )*
                    } )*
                }
            }
            fn get(d: &mut $crate::Dec<'_>) -> Result<Self, $crate::WireError> {
                let tag = <u8 as $crate::Wire>::get(d)?;
                match tag {
                    $( $tag => Ok($req::$v { $($a: $crate::Wire::get(d)?),* }), )*
                    tag => Err($crate::WireError::Tag { what: stringify!($req), tag }),
                }
            }
        }

        impl $req {
            /// The request's name, for logs.
            #[must_use]
            pub fn name(&self) -> &'static str {
                match self {
                    $( $req::$v { .. } => stringify!($v), )*
                }
            }
        }

        /// What the server implements.
        #[allow(clippy::too_many_arguments)]
        pub trait $svc {
            $( $(#[$m])* fn $f(&mut self, $($a: $t),*) -> $r; )*
        }

        /// Run a request; the encoded answer.
        pub fn $disp(s: &mut (impl $svc + ?Sized), req: $req) -> Vec<u8> {
            match req {
                $( $req::$v { $($a),* } => $crate::encode(&s.$f($($a),*)), )*
            }
        }

        #[allow(clippy::too_many_arguments)]
        /// What a client sends through: `call` moves one request and
        /// its encoded answer.
        pub trait $cli {
            /// Send a request; the encoded answer.
            fn call(&mut self, req: &$req) -> Result<Vec<u8>, $crate::WireError>;
            $(
                $(#[$m])*
                fn $f(&mut self, $($a: $t),*) -> Result<$r, $crate::WireError> {
                    let bytes = self.call(&$req::$v { $($a),* })?;
                    $crate::decode(&bytes)
                }
            )*
        }
    };
}

#[cfg(test)]
mod tests {
    crate::rpc! {
        request Req, service Svc, client Cli, dispatch run;
        /// Echo.
        0 => fn echo / Echo (text: String) -> String;
        /// Add.
        1 => fn add / Add (a: u32, b: u32) -> Option<u32>;
        /// Nothing.
        2 => fn ping / Ping () -> ();
    }

    struct Server(u32);

    impl Svc for Server {
        fn echo(&mut self, text: String) -> String {
            self.0 += 1;
            text
        }
        fn add(&mut self, a: u32, b: u32) -> Option<u32> {
            self.0 += 1;
            a.checked_add(b)
        }
        fn ping(&mut self) {}
    }

    struct Loop(Server);

    impl Cli for Loop {
        fn call(&mut self, req: &Req) -> Result<Vec<u8>, crate::WireError> {
            let back: Req = crate::decode(&crate::encode(req))?;
            Ok(run(&mut self.0, back))
        }
    }

    #[test]
    fn calls() {
        let mut c = Loop(Server(0));
        assert_eq!(c.echo("QSYS".into()).unwrap(), "QSYS");
        assert_eq!(c.add(2, 3).unwrap(), Some(5));
        assert_eq!(c.add(u32::MAX, 1).unwrap(), None);
        c.ping().unwrap();
        assert_eq!(c.0.0, 3);
        assert_eq!(Req::Ping {}.name(), "Ping");
    }
}
