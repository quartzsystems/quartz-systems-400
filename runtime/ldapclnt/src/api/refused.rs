// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The exports QS/400 does not build (docs/DIRSRV.md §14): Kerberos,
//! PKCS #11 and FIPS mode, cipher strings (`QSSLCSL` chooses), plug-ins,
//! server location by DNS, locales, friendly names, the rebind
//! callback, IBM's transactions and IBM's own requests and controls.
//! Each answers `LDAP_NOT_SUPPORTED`: an `int` function returns it, a
//! pointer function returns null with it as the error, a `void` one
//! records it.

use mi::Job;

use super::{null, ret, with};
use crate::consts::LDAP_NOT_SUPPORTED;

/// The names, as exported.
pub const NAMES: &[&str] = &[
    "ldap_create_abort_transaction_request",
    "ldap_create_account_status_request",
    "ldap_create_chaining_audit_control",
    "ldap_create_commit_transaction_request",
    "ldap_create_effective_pwdpolicy_request",
    "ldap_create_get_file_request",
    "ldap_create_group_control",
    "ldap_create_group_control_from_list",
    "ldap_create_group_eval_request",
    "ldap_create_limit_num_values_control",
    "ldap_create_locate_entry_request",
    "ldap_create_modify_groups_only_control",
    "ldap_create_omit_group_referential_integrity_control",
    "ldap_create_online_backup_request",
    "ldap_create_password_policy_bind_finalize_request",
    "ldap_create_password_policy_bind_init_request",
    "ldap_create_prepare_transaction_request",
    "ldap_create_resume_role_request",
    "ldap_create_return_deleted_objects_control",
    "ldap_create_transaction_control",
    "ldap_create_user_type_request",
    "ldap_end_transaction",
    "ldap_end_transaction_s",
    "ldap_enetwork_domain_get",
    "ldap_enetwork_domain_set",
    "ldap_free_friendlymap",
    "ldap_free_limit_num_values_response",
    "ldap_friendly_name",
    "ldap_get_locale",
    "ldap_get_tran_id",
    "ldap_krb_cc_destroy",
    "ldap_krb_ctx_destroy",
    "ldap_krb_init_tkt",
    "ldap_parse_effective_pwdpolicy_response",
    "ldap_parse_limit_num_values_response",
    "ldap_plugin_pblock_free",
    "ldap_plugin_pblock_get",
    "ldap_plugin_pblock_set",
    "ldap_plugin_sasl_bind_s",
    "ldap_prepare_transaction",
    "ldap_prepare_transaction_s",
    "ldap_proxy_dn_prefix",
    "ldap_proxy_dn_prefix_utf8",
    "ldap_server_conf_save",
    "ldap_server_free_list",
    "ldap_server_locate",
    "ldap_set_cipher",
    "ldap_set_locale",
    "ldap_set_rebind_proc",
    "ldap_ssl_pkcs11_client_init",
    "ldap_ssl_pkcs11_environment_init",
    "ldap_ssl_set_fips_mode_np",
    "ldap_start_transaction",
    "ldap_start_transaction_s",
    "ldap_url_parse_internal",
    "ldap_xlate_local_to_utf8_wrp",
];

/// An `int` function: `LDAP_NOT_SUPPORTED`.
pub fn int(j: &mut Job) -> i64 {
    with(j, |s, _| s.errno = LDAP_NOT_SUPPORTED);
    ret(LDAP_NOT_SUPPORTED)
}

/// A pointer function: null, with `LDAP_NOT_SUPPORTED` as the error.
pub fn pointer(j: &mut Job) -> u128 {
    with(j, |s, _| s.errno = LDAP_NOT_SUPPORTED);
    null()
}

/// A `void` function: the error recorded.
pub fn void(j: &mut Job) -> i64 {
    with(j, |s, _| s.errno = LDAP_NOT_SUPPORTED);
    0
}
