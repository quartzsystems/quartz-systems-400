// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One `sysv64` entry per export: C linkage (`ABI.md` §1), an `int` as
//! `i64`, a pointer as the fat pointer's `u128`, and an `i64` pad where
//! a pointer would start in the last argument register (`_p`). A test
//! checks every entry's arguments against `protos.txt`.

use crate::api::{
    bind, control, dn, entry, err, exop, lber, option, refused, result, search, session, update,
    url, xlate,
};

macro_rules! entries {
    ($( $export:literal => $name:ident ( $($a:ident : $t:ty),* ) -> $r:ty = $f:path [ $($arg:expr),* ] ; )*) => {
        $(
            #[allow(clippy::too_many_arguments, clippy::used_underscore_binding)]
            extern "sysv64" fn $name(env: *mut codegen::abi::Env, _argc: i64, _od: i64, $($a: $t),*) -> $r {
                let j = mi::storage::job_of(env);
                $f(j, $($arg),*)
            }
        )*

        /// Every entry: export name, code address, argument types.
        #[must_use]
        pub fn table() -> Vec<(&'static str, usize, &'static str)> {
            vec![ $( ($export, $name as *const () as usize, concat!($(stringify!($t), " "),*)) ),* ]
        }
    };
}

entries! {
    // Sessions
    "ldap_init" => e_init(h: u128, port: i64) -> u128 = session::init[h, port];
    "ldap_open" => e_open(h: u128, port: i64) -> u128 = session::open[h, port];
    "ldap_unbind" => e_unbind(ld: u128) -> i64 = session::unbind[ld];
    "ldap_unbind_s" => e_unbind_s(ld: u128) -> i64 = session::unbind[ld];
    "ldap_unbind_ext" => e_unbind_ext(ld: u128, _p: i64, sc: u128, cc: u128) -> i64 = session::unbind_ext[ld, sc, cc];
    "ldap_version" => e_version(v: u128) -> i64 = session::version[v];
    "ldap_set_option" => e_set_option(ld: u128, o: i64, v: u128) -> i64 = option::set_option[ld, o, v];
    "ldap_get_option" => e_get_option(ld: u128, o: i64, v: u128) -> i64 = option::get_option[ld, o, v];
    "ldap_default_dn_set" => e_default_dn_set(ld: u128, _p: i64, d: u128) -> i64 = bind::default_dn_set[ld, d];
    "ldap_default_dn_get" => e_default_dn_get(ld: u128, _p: i64, d: u128) -> i64 = bind::default_dn_get[ld, d];
    "ldap_password_set" => e_password_set(ld: u128, _p: i64, d: u128) -> i64 = bind::password_set[ld, d];
    "ldap_password_get" => e_password_get(ld: u128, _p: i64, d: u128) -> i64 = bind::password_get[ld, d];
    // Secure connections
    "ldap_ssl_init" => e_ssl_init(h: u128, port: i64, n: u128) -> u128 = session::ssl_init[h, port, n];
    "ldap_ssl_client_init" => e_ssl_client_init(k: u128, _p: i64, pw: u128, t: i64, r: u128) -> i64 = session::ssl_client_init[k, pw, t, r];
    "ldap_ssl_environment_init" => e_ssl_env_init(k: u128, _p: i64, pw: u128, st: u128, t: i64, r: u128) -> i64 = session::ssl_environment_init[k, pw, st, t, r];
    "ldap_ssl_start" => e_ssl_start(ld: u128, _p: i64, k: u128, pw: u128, n: u128) -> i64 = session::ssl_start[ld, k, pw, n];
    "ldap_app_ssl_client_init_np" => e_app_client_init(a: u128, _p: i64, r: u128) -> i64 = session::app_ssl_client_init[a, r];
    "ldap_app_ssl_init_np" => e_app_ssl_init(h: u128, port: i64, a: u128) -> u128 = session::app_ssl_init[h, port, a];
    "ldap_app_ssl_start_np" => e_app_ssl_start(ld: u128, _p: i64, a: u128) -> i64 = session::app_ssl_start[ld, a];
    "ldap_start_tls_s_np" => e_start_tls(ld: u128, _p: i64, n: u128) -> i64 = session::start_tls_s[ld, n];
    "ldap_start_tls_app_np" => e_start_tls_app(ld: u128, _p: i64, a: u128) -> i64 = session::start_tls_app[ld, a];
    "ldap_stop_tls_s_np" => e_stop_tls(ld: u128) -> i64 = session::stop_tls_s[ld];
    // Binds
    "ldap_simple_bind" => e_simple_bind(ld: u128, _p: i64, d: u128, pw: u128) -> i64 = bind::simple_bind[ld, d, pw];
    "ldap_simple_bind_s" => e_simple_bind_s(ld: u128, _p: i64, d: u128, pw: u128) -> i64 = bind::simple_bind_s[ld, d, pw];
    "ldap_bind" => e_bind(ld: u128, _p: i64, d: u128, c: u128, m: i64) -> i64 = bind::bind[ld, d, c, m];
    "ldap_bind_s" => e_bind_s(ld: u128, _p: i64, d: u128, c: u128, m: i64) -> i64 = bind::bind_s[ld, d, c, m];
    "ldap_sasl_bind" => e_sasl_bind(ld: u128, _p: i64, d: u128, m: u128, c: u128, sc: u128, cc: u128, id: u128) -> i64 = bind::sasl_bind[ld, d, m, c, sc, cc, id];
    "ldap_sasl_bind_s" => e_sasl_bind_s(ld: u128, _p: i64, d: u128, m: u128, c: u128, sc: u128, cc: u128, cr: u128) -> i64 = bind::sasl_bind_s[ld, d, m, c, sc, cc, cr];
    "ldap_parse_sasl_bind_result" => e_parse_sasl(ld: u128, _p: i64, r: u128, c: u128, f: i64) -> i64 = result::parse_sasl_bind_result[ld, r, c, f];
    "ldap_get_bind_controls" => e_bind_ctrls(ld: u128, _p: i64, o: u128) -> i64 = control::get_bind_controls[ld, o];
    // Searches
    "ldap_search" => e_search(ld: u128, _p: i64, b: u128, s: i64, f: u128, a: u128, o: i64) -> i64 = search::search[ld, b, s, f, a, o];
    "ldap_search_s" => e_search_s(ld: u128, _p: i64, b: u128, s: i64, f: u128, a: u128, o: i64, r: u128) -> i64 = search::search_s[ld, b, s, f, a, o, r];
    "ldap_search_st" => e_search_st(ld: u128, _p: i64, b: u128, s: i64, f: u128, a: u128, o: i64, t: u128, r: u128) -> i64 = search::search_st[ld, b, s, f, a, o, t, r];
    "ldap_search_ext" => e_search_ext(ld: u128, _p: i64, b: u128, s: i64, f: u128, a: u128, o: i64, sc: u128, cc: u128, t: u128, z: i64, id: u128) -> i64 = search::search_ext[ld, b, s, f, a, o, sc, cc, t, z, id];
    "ldap_search_ext_s" => e_search_ext_s(ld: u128, _p: i64, b: u128, s: i64, f: u128, a: u128, o: i64, sc: u128, cc: u128, t: u128, z: i64, r: u128) -> i64 = search::search_ext_s[ld, b, s, f, a, o, sc, cc, t, z, r];
    "ldap_url_search" => e_url_search(ld: u128, _p: i64, u: u128, o: i64) -> i64 = search::url_search[ld, u, o];
    "ldap_url_search_s" => e_url_search_s(ld: u128, _p: i64, u: u128, o: i64, r: u128) -> i64 = search::url_search_s[ld, u, o, r];
    "ldap_url_search_st" => e_url_search_st(ld: u128, _p: i64, u: u128, o: i64, t: u128, r: u128) -> i64 = search::url_search_st[ld, u, o, t, r];
    // Results
    "ldap_result" => e_result(ld: u128, id: i64, all: i64, t: u128, r: u128) -> i64 = result::result[ld, id, all, t, r];
    "ldap_msgfree" => e_msgfree(m: u128) -> i64 = result::msgfree[m];
    "ldap_msgtype" => e_msgtype(m: u128) -> i64 = result::msgtype[m];
    "ldap_msgid" => e_msgid(m: u128) -> i64 = result::msgid[m];
    "ldap_result2error" => e_result2error(ld: u128, _p: i64, r: u128, f: i64) -> i64 = result::result2error[ld, r, f];
    "ldap_parse_result" => e_parse_result(ld: u128, _p: i64, r: u128, c: u128, m: u128, e: u128, rf: u128, sc: u128, f: i64) -> i64 = result::parse_result[ld, r, c, m, e, rf, sc, f];
    "ldap_parse_extended_result" => e_parse_ext(ld: u128, _p: i64, r: u128, o: u128, d: u128, f: i64) -> i64 = result::parse_extended_result[ld, r, o, d, f];
    "ldap_parse_extended_result_w_controls" => e_parse_ext_c(ld: u128, _p: i64, r: u128, o: u128, d: u128, sc: u128, f: i64) -> i64 = result::parse_extended[ld, r, o, d, sc, f];
    "ldap_parse_reference" => e_parse_ref(ld: u128, _p: i64, r: u128, rf: u128, sc: u128, f: i64) -> i64 = result::parse_reference[ld, r, rf, sc, f];
    "ldap_parse_reference_np" => e_parse_ref_np(ld: u128, _p: i64, r: u128, rf: u128, sc: u128, f: i64) -> i64 = result::parse_reference[ld, r, rf, sc, f];
    "ldap_first_message" => e_first_message(ld: u128, _p: i64, r: u128) -> u128 = entry::first_message[ld, r];
    "ldap_next_message" => e_next_message(ld: u128, _p: i64, r: u128) -> u128 = entry::next_message[ld, r];
    "ldap_count_messages" => e_count_messages(ld: u128, _p: i64, r: u128) -> i64 = entry::count_messages[ld, r];
    "ldap_first_entry" => e_first_entry(ld: u128, _p: i64, r: u128) -> u128 = entry::first_entry[ld, r];
    "ldap_next_entry" => e_next_entry(ld: u128, _p: i64, r: u128) -> u128 = entry::next_entry[ld, r];
    "ldap_count_entries" => e_count_entries(ld: u128, _p: i64, r: u128) -> i64 = entry::count_entries[ld, r];
    "ldap_first_reference" => e_first_reference(ld: u128, _p: i64, r: u128) -> u128 = entry::first_reference[ld, r];
    "ldap_next_reference" => e_next_reference(ld: u128, _p: i64, r: u128) -> u128 = entry::next_reference[ld, r];
    "ldap_count_references" => e_count_references(ld: u128, _p: i64, r: u128) -> i64 = entry::count_references[ld, r];
    // Entries
    "ldap_get_dn" => e_get_dn(ld: u128, _p: i64, e: u128) -> u128 = entry::get_dn[ld, e];
    "ldap_first_attribute" => e_first_attribute(ld: u128, _p: i64, e: u128, b: u128) -> u128 = entry::first_attribute[ld, e, b];
    "ldap_next_attribute" => e_next_attribute(ld: u128, _p: i64, e: u128, b: u128) -> u128 = entry::next_attribute[ld, e, b];
    "ldap_count_attributes" => e_count_attributes(ld: u128, _p: i64, e: u128) -> i64 = entry::count_attributes[ld, e];
    "ldap_get_values" => e_get_values(ld: u128, _p: i64, e: u128, a: u128) -> u128 = entry::get_values[ld, e, a];
    "ldap_get_values_len" => e_get_values_len(ld: u128, _p: i64, e: u128, a: u128) -> u128 = entry::get_values_len[ld, e, a];
    "ldap_count_values" => e_count_values(v: u128) -> i64 = entry::count_values[v];
    "ldap_count_values_len" => e_count_values_len(v: u128) -> i64 = entry::count_values[v];
    "ldap_value_free" => e_value_free(v: u128) -> i64 = entry::value_free[v];
    "ldap_value_free_len" => e_value_free_len(v: u128) -> i64 = entry::value_free_len[v];
    "ldap_get_entry_controls" => e_entry_ctrls(ld: u128, _p: i64, e: u128, o: u128) -> i64 = entry::get_entry_controls[ld, e, o];
    "ldap_get_entry_controls_np" => e_entry_ctrls_np(ld: u128, _p: i64, e: u128, o: u128) -> i64 = entry::get_entry_controls[ld, e, o];
    "ldap_memfree" => e_memfree(m: u128) -> i64 = entry::memfree[m];
    "ldap_ber_free" => e_ldap_ber_free(b: u128, f: i64) -> i64 = entry::ber_free[b, f];
    "ldap_mods_free" => e_mods_free(m: u128, f: i64) -> i64 = entry::mods_free[m, f];
    // DNs
    "ldap_explode_dn" => e_explode_dn(d: u128, n: i64) -> u128 = dn::explode_dn[d, n];
    "ldap_explode_dn_utf8" => e_explode_dn_utf8(d: u128, n: i64) -> u128 = dn::explode_dn_utf8[d, n];
    "ldap_explode_rdn" => e_explode_rdn(d: u128, n: i64) -> u128 = dn::explode_rdn[d, n];
    "ldap_explode_rdn_utf8" => e_explode_rdn_utf8(d: u128, n: i64) -> u128 = dn::explode_rdn_utf8[d, n];
    "ldap_explode_dns" => e_explode_dns(d: u128) -> u128 = dn::explode_dns[d];
    "ldap_dn2ufn" => e_dn2ufn(d: u128) -> u128 = dn::dn2ufn[d];
    // Updates
    "ldap_add" => e_add(ld: u128, _p: i64, d: u128, m: u128) -> i64 = update::add[ld, d, m];
    "ldap_add_s" => e_add_s(ld: u128, _p: i64, d: u128, m: u128) -> i64 = update::add_s[ld, d, m];
    "ldap_add_ext" => e_add_ext(ld: u128, _p: i64, d: u128, m: u128, sc: u128, cc: u128, id: u128) -> i64 = update::add_ext[ld, d, m, sc, cc, id];
    "ldap_add_ext_s" => e_add_ext_s(ld: u128, _p: i64, d: u128, m: u128, sc: u128, cc: u128) -> i64 = update::add_ext_s[ld, d, m, sc, cc];
    "ldap_modify" => e_modify(ld: u128, _p: i64, d: u128, m: u128) -> i64 = update::modify[ld, d, m];
    "ldap_modify_s" => e_modify_s(ld: u128, _p: i64, d: u128, m: u128) -> i64 = update::modify_s[ld, d, m];
    "ldap_modify_ext" => e_modify_ext(ld: u128, _p: i64, d: u128, m: u128, sc: u128, cc: u128, id: u128) -> i64 = update::modify_ext[ld, d, m, sc, cc, id];
    "ldap_modify_ext_s" => e_modify_ext_s(ld: u128, _p: i64, d: u128, m: u128, sc: u128, cc: u128) -> i64 = update::modify_ext_s[ld, d, m, sc, cc];
    "ldap_delete" => e_delete(ld: u128, _p: i64, d: u128) -> i64 = update::delete[ld, d];
    "ldap_delete_s" => e_delete_s(ld: u128, _p: i64, d: u128) -> i64 = update::delete_s[ld, d];
    "ldap_delete_ext" => e_delete_ext(ld: u128, _p: i64, d: u128, sc: u128, cc: u128, id: u128) -> i64 = update::delete_ext[ld, d, sc, cc, id];
    "ldap_delete_ext_s" => e_delete_ext_s(ld: u128, _p: i64, d: u128, sc: u128, cc: u128) -> i64 = update::delete_ext_s[ld, d, sc, cc];
    "ldap_modrdn" => e_modrdn(ld: u128, _p: i64, d: u128, r: u128, x: i64) -> i64 = update::modrdn[ld, d, r, x];
    "ldap_modrdn_s" => e_modrdn_s(ld: u128, _p: i64, d: u128, r: u128, x: i64) -> i64 = update::modrdn_s[ld, d, r, x];
    "ldap_rename" => e_rename(ld: u128, _p: i64, d: u128, r: u128, np: u128, x: i64, sc: u128, cc: u128, id: u128) -> i64 = update::rename[ld, d, r, np, x, sc, cc, id];
    "ldap_rename_s" => e_rename_s(ld: u128, _p: i64, d: u128, r: u128, np: u128, x: i64, sc: u128, cc: u128) -> i64 = update::rename_s[ld, d, r, np, x, sc, cc];
    "ldap_compare" => e_compare(ld: u128, _p: i64, d: u128, a: u128, v: u128) -> i64 = update::compare[ld, d, a, v];
    "ldap_compare_s" => e_compare_s(ld: u128, _p: i64, d: u128, a: u128, v: u128) -> i64 = update::compare_s[ld, d, a, v];
    "ldap_compare_ext" => e_compare_ext(ld: u128, _p: i64, d: u128, a: u128, v: u128, sc: u128, cc: u128, id: u128) -> i64 = update::compare_ext[ld, d, a, v, sc, cc, id];
    "ldap_compare_ext_s" => e_compare_ext_s(ld: u128, _p: i64, d: u128, a: u128, v: u128, sc: u128, cc: u128) -> i64 = update::compare_ext_s[ld, d, a, v, sc, cc];
    "ldap_abandon" => e_abandon(ld: u128, id: i64) -> i64 = update::abandon[ld, id];
    "ldap_abandon_ext" => e_abandon_ext(ld: u128, id: i64, sc: u128, cc: u128) -> i64 = update::abandon_ext[ld, id, sc, cc];
    "ldap_extended_operation" => e_exop(ld: u128, _p: i64, o: u128, d: u128, sc: u128, cc: u128, id: u128) -> i64 = exop::extended_operation[ld, o, d, sc, cc, id];
    "ldap_extended_operation_s" => e_exop_s(ld: u128, _p: i64, o: u128, d: u128, sc: u128, cc: u128, ro: u128, rd: u128) -> i64 = exop::extended_operation_s[ld, o, d, sc, cc, ro, rd];
    // Controls
    "ldap_create_control" => e_create_control(o: u128, _p: i64, b: u128, c: i64, out: u128) -> i64 = control::create_control[o, b, c, out];
    "ldap_create_page_control" => e_create_page(ld: u128, n: i64, ck: u128, c: i64, out: u128) -> i64 = control::create_page_control[ld, n, ck, c, out];
    "ldap_parse_page_control" => e_parse_page(ld: u128, _p: i64, l: u128, t: u128, ck: u128) -> i64 = control::parse_page_control[ld, l, t, ck];
    "ldap_create_sort_keylist" => e_sort_keylist(out: u128, _p: i64, k: u128) -> i64 = control::create_sort_keylist[out, k];
    "ldap_free_sort_keylist" => e_free_keylist(l: u128) -> i64 = control::free_sort_keylist[l];
    "ldap_create_sort_control" => e_create_sort(ld: u128, _p: i64, l: u128, c: i64, out: u128) -> i64 = control::create_sort_control[ld, l, c, out];
    "ldap_parse_sort_control" => e_parse_sort(ld: u128, _p: i64, l: u128, r: u128, a: u128) -> i64 = control::parse_sort_control[ld, l, r, a];
    "ldap_create_proxyauth_control" => e_create_proxy(ld: u128, _p: i64, d: u128, c: i64, out: u128) -> i64 = control::create_proxyauth_control[ld, d, c, out];
    "ldap_create_persistentsearch_control" => e_create_psearch(ld: u128, t: i64, o: i64, e: i64, c: i64, out: u128) -> i64 = control::create_persistentsearch_control[ld, t, o, e, c, out];
    "ldap_parse_entrychange_control" => e_parse_ec(ld: u128, _p: i64, l: u128, t: u128, pd: u128, h: u128, n: u128) -> i64 = control::parse_entrychange_control[ld, l, t, pd, h, n];
    "ldap_create_assertion_control" => e_create_assert(ld: u128, _p: i64, f: u128, c: i64, out: u128) -> i64 = control::create_assertion_control[ld, f, c, out];
    "ldap_create_vlv_control" => e_create_vlv(ld: u128, _p: i64, i: u128, out: u128) -> i64 = control::create_vlv_control[ld, i, out];
    "ldap_parse_vlv_control" => e_parse_vlv(ld: u128, _p: i64, l: u128, t: u128, n: u128, cx: u128, e: u128) -> i64 = control::parse_vlv_control[ld, l, t, n, cx, e];
    "ldap_parse_pwdpolicy_response" => e_parse_pp(l: u128, _p: i64, w: u128, v: u128, e: u128) -> i64 = control::parse_pwdpolicy_response[l, w, v, e];
    "ldap_pwdpolicy_err2string" => e_pp_text(e: i64) -> u128 = err::pwdpolicy_err2string[e];
    "ldap_control_free" => e_control_free(c: u128) -> i64 = control::control_free[c];
    "ldap_controls_free" => e_controls_free(c: u128) -> i64 = control::controls_free[c];
    "ldap_copy_controls" => e_copy_controls(to: u128, _p: i64, from: u128) -> i64 = control::copy_controls[to, from];
    "ldap_insert_control" => e_insert_control(c: u128, _p: i64, l: u128) -> i64 = control::insert_control[c, l];
    "ldap_add_control" => e_add_control(o: u128, n: i64, v: u128, c: i64, l: u128) -> i64 = control::add_control[o, n, v, c, l];
    "ldap_remove_control" => e_remove_control(c: u128, _p: i64, l: u128, f: i64) -> i64 = control::remove_control[c, l, f];
    "ldap_merge_control_lists" => e_merge_controls(a: u128, _p: i64, b: u128, out: u128) -> i64 = control::merge_control_lists[a, b, out];
    // Errors
    "ldap_err2string" => e_err2string(e: i64) -> u128 = err::err2string[e];
    "ldap_perror" => e_perror(ld: u128, _p: i64, s: u128) -> i64 = err::perror[ld, s];
    "ldap_get_lderrno" => e_get_lderrno(ld: u128, _p: i64, m: u128, s: u128) -> i64 = err::get_lderrno[ld, m, s];
    "ldap_set_lderrno" => e_set_lderrno(ld: u128, e: i64, m: u128, s: u128) -> i64 = err::set_lderrno[ld, e, m, s];
    "ldap_get_errno" => e_get_errno(ld: u128) -> i64 = err::get_errno[ld];
    "ldap_get_exterror" => e_get_exterror(ld: u128) -> i64 = err::get_exterror[ld];
    // URLs
    "ldap_is_ldap_url" => e_is_url(u: u128) -> i64 = url::is_ldap_url[u];
    "ldap_is_ldap_url_utf8" => e_is_url_utf8(u: u128) -> i64 = url::is_ldap_url_utf8[u];
    "ldap_url_parse" => e_url_parse(u: u128, _p: i64, o: u128) -> i64 = url::url_parse[u, o];
    "ldap_url_parse_utf8" => e_url_parse_utf8(u: u128, _p: i64, o: u128) -> i64 = url::url_parse_utf8[u, o];
    "ldap_free_urldesc" => e_free_urldesc(d: u128) -> i64 = url::free_urldesc[d];
    // Code pages
    "ldap_set_iconv_local_codepage" => e_set_cp(c: i64) -> i64 = xlate::set_codepage[c];
    "ldap_set_iconv_local_charset" => e_set_cs(c: u128) -> i64 = xlate::set_charset[c];
    "ldap_get_iconv_local_codepage" => e_get_cp() -> i64 = xlate::get_codepage[];
    "ldap_xlate_local_to_utf8" => e_l2u8(i: u128, _p: i64, il: u128, o: u128, ol: u128) -> i64 = xlate::local_to_utf8[i, il, o, ol];
    "ldap_xlate_utf8_to_local" => e_u82l(i: u128, _p: i64, il: u128, o: u128, ol: u128) -> i64 = xlate::utf8_to_local[i, il, o, ol];
    "ldap_xlate_local_to_unicode" => e_l2u(i: u128, _p: i64, il: u128, o: u128, ol: u128) -> i64 = xlate::local_to_unicode[i, il, o, ol];
    "ldap_xlate_unicode_to_local" => e_u2l(i: u128, _p: i64, il: u128, o: u128, ol: u128) -> i64 = xlate::unicode_to_local[i, il, o, ol];
    "ldap_char_size" => e_char_size(p: u128) -> i64 = xlate::char_size[p];
    // LBER
    "ber_alloc_t" => e_ber_alloc(o: i64) -> u128 = lber::alloc_t[o];
    "ber_init" => e_ber_init(b: u128) -> u128 = lber::init[b];
    "ber_init2" => e_ber_init2(b: u128, _p: i64, v: u128, o: i64) -> i64 = lber::init2[b, v, o];
    "ber_flatten" => e_ber_flatten(b: u128, _p: i64, o: u128) -> i64 = lber::flatten[b, o];
    "ber_free" => e_ber_free(b: u128, f: i64) -> i64 = lber::free[b, f];
    "ber_peek_tag" => e_ber_peek(b: u128, _p: i64, l: u128) -> i64 = lber::peek_tag[b, l];
    "ber_skip_tag" => e_ber_skip(b: u128, _p: i64, l: u128) -> i64 = lber::skip_tag[b, l];
    "ber_get_tag" => e_ber_get_tag(b: u128) -> i64 = lber::get_tag[b];
    "ber_skip_element" => e_ber_skip_el(b: u128, _p: i64, v: u128) -> i64 = lber::skip_element[b, v];
    "ber_first_element" => e_ber_first(b: u128, _p: i64, l: u128, x: u128) -> i64 = lber::first_element[b, l, x];
    "ber_next_element" => e_ber_next(b: u128, _p: i64, l: u128, x: u128) -> i64 = lber::next_element[b, l, x];
    "ber_bvdup" => e_bvdup(b: u128) -> u128 = lber::bvdup[b];
    "ber_bvfree" => e_bvfree(b: u128) -> i64 = lber::bvfree[b];
    "ber_bvecfree" => e_bvecfree(b: u128) -> i64 = lber::bvecfree[b];
}

/// How many stack words a variadic entry reads.
const WORDS: usize = 24;

/// The format string of a variadic call, in the local code page.
fn format(j: &mut mi::Job, fmt: u128) -> Option<String> {
    let b = crate::mem::cbytes(j, fmt)?;
    let c = crate::api::with(j, |s, _| s.codec());
    Some(c.text(&b))
}

macro_rules! variadic {
    ($name:ident, $printf:expr, $f:path) => {
        #[allow(clippy::too_many_arguments)]
        extern "sysv64" fn $name(
            env: *mut codegen::abi::Env,
            _argc: i64,
            _od: i64,
            ber: u128,
            _p: i64,
            fmt: u128,
            w0: i64,
            w1: i64,
            w2: i64,
            w3: i64,
            w4: i64,
            w5: i64,
            w6: i64,
            w7: i64,
            w8: i64,
            w9: i64,
            w10: i64,
            w11: i64,
            w12: i64,
            w13: i64,
            w14: i64,
            w15: i64,
            w16: i64,
            w17: i64,
            w18: i64,
            w19: i64,
            w20: i64,
            w21: i64,
            w22: i64,
            w23: i64,
        ) -> i64 {
            let j = mi::storage::job_of(env);
            let words: [i64; WORDS] = [
                w0, w1, w2, w3, w4, w5, w6, w7, w8, w9, w10, w11, w12, w13, w14, w15, w16, w17,
                w18, w19, w20, w21, w22, w23,
            ];
            let Some(f) = format(j, fmt) else {
                return -1;
            };
            // `fmt` takes stack slots 0 and 1; the list starts at 2.
            let args = lber::args_from(&f, $printf, &words, 2);
            $f(j, ber, &f, &args)
        }
    };
}

variadic!(e_ber_printf, true, lber::printf);
variadic!(e_ber_scanf, false, lber::scanf);

macro_rules! refuse {
    ($( $export:literal => $kind:ident ),* $(,)?) => {
        /// The refused exports and their entries.
        fn refused_table() -> Vec<(&'static str, usize)> {
            vec![ $( ($export, refuse!(@entry $kind)) ),* ]
        }
    };
    (@entry int) => { r_int as *const () as usize };
    (@entry ptr) => { r_ptr as *const () as usize };
    (@entry void) => { r_void as *const () as usize };
}

extern "sysv64" fn r_int(env: *mut codegen::abi::Env, _argc: i64, _od: i64) -> i64 {
    refused::int(mi::storage::job_of(env))
}

extern "sysv64" fn r_ptr(env: *mut codegen::abi::Env, _argc: i64, _od: i64) -> u128 {
    refused::pointer(mi::storage::job_of(env))
}

extern "sysv64" fn r_void(env: *mut codegen::abi::Env, _argc: i64, _od: i64) -> i64 {
    refused::void(mi::storage::job_of(env))
}

refuse! {
    "ldap_create_abort_transaction_request" => int,
    "ldap_create_account_status_request" => int,
    "ldap_create_chaining_audit_control" => int,
    "ldap_create_commit_transaction_request" => int,
    "ldap_create_effective_pwdpolicy_request" => int,
    "ldap_create_get_file_request" => int,
    "ldap_create_group_control" => int,
    "ldap_create_group_control_from_list" => int,
    "ldap_create_group_eval_request" => int,
    "ldap_create_limit_num_values_control" => int,
    "ldap_create_locate_entry_request" => int,
    "ldap_create_modify_groups_only_control" => int,
    "ldap_create_omit_group_referential_integrity_control" => int,
    "ldap_create_online_backup_request" => int,
    "ldap_create_password_policy_bind_finalize_request" => int,
    "ldap_create_password_policy_bind_init_request" => int,
    "ldap_create_prepare_transaction_request" => int,
    "ldap_create_resume_role_request" => int,
    "ldap_create_return_deleted_objects_control" => int,
    "ldap_create_transaction_control" => int,
    "ldap_create_user_type_request" => int,
    "ldap_end_transaction" => int,
    "ldap_end_transaction_s" => int,
    "ldap_enetwork_domain_get" => int,
    "ldap_enetwork_domain_set" => int,
    "ldap_free_friendlymap" => void,
    "ldap_free_limit_num_values_response" => void,
    "ldap_friendly_name" => ptr,
    "ldap_get_locale" => ptr,
    "ldap_get_tran_id" => int,
    "ldap_krb_cc_destroy" => int,
    "ldap_krb_ctx_destroy" => int,
    "ldap_krb_init_tkt" => int,
    "ldap_parse_effective_pwdpolicy_response" => int,
    "ldap_parse_limit_num_values_response" => int,
    "ldap_plugin_pblock_free" => void,
    "ldap_plugin_pblock_get" => int,
    "ldap_plugin_pblock_set" => int,
    "ldap_plugin_sasl_bind_s" => int,
    "ldap_prepare_transaction" => int,
    "ldap_prepare_transaction_s" => int,
    "ldap_proxy_dn_prefix" => int,
    "ldap_proxy_dn_prefix_utf8" => int,
    "ldap_server_conf_save" => int,
    "ldap_server_free_list" => void,
    "ldap_server_locate" => int,
    "ldap_set_cipher" => int,
    "ldap_set_locale" => int,
    "ldap_set_rebind_proc" => void,
    "ldap_ssl_pkcs11_client_init" => int,
    "ldap_ssl_pkcs11_environment_init" => int,
    "ldap_ssl_set_fips_mode_np" => int,
    "ldap_start_transaction" => int,
    "ldap_start_transaction_s" => int,
    "ldap_url_parse_internal" => int,
    "ldap_xlate_local_to_utf8_wrp" => int,
}

/// Every exported procedure with its code address, by IBM's name.
#[must_use]
pub fn procedures() -> Vec<(&'static str, usize)> {
    let mut v: Vec<(&'static str, usize)> = table().into_iter().map(|(n, a, _)| (n, a)).collect();
    v.push(("ber_printf", e_ber_printf as *const () as usize));
    v.push(("ber_scanf", e_ber_scanf as *const () as usize));
    v.extend(refused_table());
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::{abi_types, protos};

    /// Every built function's entry takes what its prototype gives, pads
    /// included; every refused one is a refusal; nothing is missing.
    #[test]
    fn entries_match_the_prototypes() {
        let built: std::collections::HashMap<&str, &str> =
            table().into_iter().map(|(n, _, s)| (n, s)).collect();
        let refused: std::collections::HashSet<&str> =
            refused_table().into_iter().map(|(n, _)| n).collect();
        assert_eq!(refused.len(), crate::api::refused::NAMES.len());
        for n in crate::api::refused::NAMES {
            assert!(refused.contains(n), "{n}");
        }
        for p in protos() {
            if p.varargs {
                assert!(["ber_printf", "ber_scanf"].contains(&p.name.as_str()));
                continue;
            }
            if p.refused {
                assert!(
                    refused.contains(p.name.as_str()),
                    "{} is not refused",
                    p.name
                );
                assert!(
                    !built.contains_key(p.name.as_str()),
                    "{} is built and refused",
                    p.name
                );
                continue;
            }
            let sig = built
                .get(p.name.as_str())
                .unwrap_or_else(|| panic!("{} has no entry", p.name));
            let got: Vec<&str> = sig.split_whitespace().collect();
            assert_eq!(got, abi_types(&p), "{}", p.name);
        }
        assert_eq!(procedures().len(), protos().len());
    }
}
