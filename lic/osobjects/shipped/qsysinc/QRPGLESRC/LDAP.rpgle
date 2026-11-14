**FREE
// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.
//
// LDAP: QS/400's LDAP client (service program QGLDCLNT) for ILE RPG, a
// QS/400 addition generated from the same table as QSYSINC/H(LDAP).
// Strings are in the job's CCSID unless LDAP_OPT_UTF8_IO is on.
// ber_printf and ber_scanf take a variable list and are not declared.

/if defined(QS400_LDAP_RPGLE)
/eof
/endif
/define QS400_LDAP_RPGLE

dcl-c LDAP_PORT 389;
dcl-c LDAPS_PORT 636;
dcl-c LDAP_VERSION2 2;
dcl-c LDAP_VERSION3 3;
dcl-c LDAP_VERSION_MIN 2;
dcl-c LDAP_VERSION_MAX 3;
dcl-c LDAP_NO_LIMIT 0;
dcl-c LDAP_OPT_API_INFO 0;
dcl-c LDAP_OPT_DESC 1;
dcl-c LDAP_OPT_DEREF 2;
dcl-c LDAP_OPT_SIZELIMIT 3;
dcl-c LDAP_OPT_TIMELIMIT 4;
dcl-c LDAP_OPT_REFHOPLIMIT 5;
dcl-c LDAP_OPT_REBIND_FN 6;
dcl-c LDAP_OPT_REBIND_ARG 7;
dcl-c LDAP_OPT_REFERRALS 8;
dcl-c LDAP_OPT_RESTART 9;
dcl-c LDAP_OPT_SSL_CIPHER 10;
dcl-c LDAP_OPT_SSL_TIMEOUT 11;
dcl-c LDAP_OPT_DEBUG 12;
dcl-c LDAP_OPT_UTF8_IO 14;
dcl-c LDAP_OPT_PROTOCOL_VERSION 17;
dcl-c LDAP_OPT_SERVER_CONTROLS 18;
dcl-c LDAP_OPT_CLIENT_CONTROLS 19;
dcl-c LDAP_OPT_HOST_NAME 48;
dcl-c LDAP_OPT_ERROR_NUMBER 49;
dcl-c LDAP_OPT_ERROR_STRING 50;
dcl-c LDAP_OPT_MATCHED_DN 51;
dcl-c LDAP_OPT_EXT_ERROR 52;
dcl-c LDAP_OPT_ON 1;
dcl-c LDAP_OPT_OFF 0;
dcl-c LDAP_DEREF_NEVER 0;
dcl-c LDAP_DEREF_SEARCHING 1;
dcl-c LDAP_DEREF_FINDING 2;
dcl-c LDAP_DEREF_ALWAYS 3;
dcl-c LDAP_SCOPE_BASE 0;
dcl-c LDAP_SCOPE_ONELEVEL 1;
dcl-c LDAP_SCOPE_SUBTREE 2;
dcl-c LDAP_MOD_ADD 0;
dcl-c LDAP_MOD_DELETE 1;
dcl-c LDAP_MOD_REPLACE 2;
dcl-c LDAP_MOD_INCREMENT 3;
dcl-c LDAP_MOD_BVALUES 128;
dcl-c LDAP_AUTH_NONE 0;
dcl-c LDAP_AUTH_SIMPLE 128;
dcl-c LDAP_AUTH_SASL 163;
dcl-c LDAP_MSG_ONE 0;
dcl-c LDAP_MSG_ALL 1;
dcl-c LDAP_MSG_RECEIVED 2;
dcl-c LDAP_RES_ANY -1;
dcl-c LDAP_RES_BIND 97;
dcl-c LDAP_RES_SEARCH_ENTRY 100;
dcl-c LDAP_RES_SEARCH_RESULT 101;
dcl-c LDAP_RES_MODIFY 103;
dcl-c LDAP_RES_ADD 105;
dcl-c LDAP_RES_DELETE 107;
dcl-c LDAP_RES_MODRDN 109;
dcl-c LDAP_RES_COMPARE 111;
dcl-c LDAP_RES_SEARCH_REFERENCE 115;
dcl-c LDAP_RES_EXTENDED 120;
dcl-c LDAP_RES_INTERMEDIATE 121;
dcl-c LDAP_SUCCESS 0;
dcl-c LDAP_OPERATIONS_ERROR 1;
dcl-c LDAP_PROTOCOL_ERROR 2;
dcl-c LDAP_TIMELIMIT_EXCEEDED 3;
dcl-c LDAP_SIZELIMIT_EXCEEDED 4;
dcl-c LDAP_COMPARE_FALSE 5;
dcl-c LDAP_COMPARE_TRUE 6;
dcl-c LDAP_STRONG_AUTH_NOT_SUPPORTED 7;
dcl-c LDAP_STRONG_AUTH_REQUIRED 8;
dcl-c LDAP_PARTIAL_RESULTS 9;
dcl-c LDAP_REFERRAL 10;
dcl-c LDAP_ADMIN_LIMIT_EXCEEDED 11;
dcl-c LDAP_UNAVAILABLE_CRITICAL_EXTENSION 12;
dcl-c LDAP_CONFIDENTIALITY_REQUIRED 13;
dcl-c LDAP_SASL_BIND_IN_PROGRESS 14;
dcl-c LDAP_NO_SUCH_ATTRIBUTE 16;
dcl-c LDAP_UNDEFINED_TYPE 17;
dcl-c LDAP_INAPPROPRIATE_MATCHING 18;
dcl-c LDAP_CONSTRAINT_VIOLATION 19;
dcl-c LDAP_TYPE_OR_VALUE_EXISTS 20;
dcl-c LDAP_INVALID_SYNTAX 21;
dcl-c LDAP_NO_SUCH_OBJECT 32;
dcl-c LDAP_ALIAS_PROBLEM 33;
dcl-c LDAP_INVALID_DN_SYNTAX 34;
dcl-c LDAP_IS_LEAF 35;
dcl-c LDAP_ALIAS_DEREF_PROBLEM 36;
dcl-c LDAP_INAPPROPRIATE_AUTH 48;
dcl-c LDAP_INVALID_CREDENTIALS 49;
dcl-c LDAP_INSUFFICIENT_ACCESS 50;
dcl-c LDAP_BUSY 51;
dcl-c LDAP_UNAVAILABLE 52;
dcl-c LDAP_UNWILLING_TO_PERFORM 53;
dcl-c LDAP_LOOP_DETECT 54;
dcl-c LDAP_NAMING_VIOLATION 64;
dcl-c LDAP_OBJECT_CLASS_VIOLATION 65;
dcl-c LDAP_NOT_ALLOWED_ON_NONLEAF 66;
dcl-c LDAP_NOT_ALLOWED_ON_RDN 67;
dcl-c LDAP_ALREADY_EXISTS 68;
dcl-c LDAP_NO_OBJECT_CLASS_MODS 69;
dcl-c LDAP_RESULTS_TOO_LARGE 70;
dcl-c LDAP_AFFECTS_MULTIPLE_DSAS 71;
dcl-c LDAP_OTHER 80;
dcl-c LDAP_SERVER_DOWN 81;
dcl-c LDAP_LOCAL_ERROR 82;
dcl-c LDAP_ENCODING_ERROR 83;
dcl-c LDAP_DECODING_ERROR 84;
dcl-c LDAP_TIMEOUT 85;
dcl-c LDAP_AUTH_UNKNOWN 86;
dcl-c LDAP_FILTER_ERROR 87;
dcl-c LDAP_USER_CANCELLED 88;
dcl-c LDAP_PARAM_ERROR 89;
dcl-c LDAP_NO_MEMORY 90;
dcl-c LDAP_CONNECT_ERROR 91;
dcl-c LDAP_NOT_SUPPORTED 92;
dcl-c LDAP_CONTROL_NOT_FOUND 93;
dcl-c LDAP_NO_RESULTS_RETURNED 94;
dcl-c LDAP_MORE_RESULTS_TO_RETURN 95;
dcl-c LDAP_CLIENT_LOOP 96;
dcl-c LDAP_REFERRAL_LIMIT_EXCEEDED 97;
dcl-c LDAP_CHANGETYPE_ADD 1;
dcl-c LDAP_CHANGETYPE_DELETE 2;
dcl-c LDAP_CHANGETYPE_MODIFY 4;
dcl-c LDAP_CHANGETYPE_MODDN 8;
dcl-c LDAP_CHANGETYPE_ANY 15;
dcl-c LDAP_PP_WARNING_NONE 0;
dcl-c LDAP_PP_TIME_BEFORE_EXPIRATION 1;
dcl-c LDAP_PP_GRACE_LOGINS_REMAINING 2;
dcl-c LDAP_PP_NO_ERROR -1;
dcl-c LDAP_PP_PASSWORD_EXPIRED 0;
dcl-c LDAP_PP_ACCOUNT_LOCKED 1;
dcl-c LDAP_PP_CHANGE_AFTER_RESET 2;
dcl-c LDAP_PP_MOD_NOT_ALLOWED 3;
dcl-c LDAP_PP_MUST_SUPPLY_OLD 4;
dcl-c LDAP_PP_INSUFFICIENT_QUALITY 5;
dcl-c LDAP_PP_TOO_SHORT 6;
dcl-c LDAP_PP_TOO_YOUNG 7;
dcl-c LDAP_PP_IN_HISTORY 8;
dcl-c LDAP_SSL_ALREADY_INITIALIZED 112;
dcl-c LDAP_SSL_INITIALIZE_FAILED 113;
dcl-c LDAP_SSL_CLIENT_INIT_NOT_CALLED 114;
dcl-c LDAP_SSL_PARAM_ERROR 115;
dcl-c LDAP_SSL_HANDSHAKE_FAILED 116;
dcl-c LDAP_SSL_GET_CIPHER_FAILED 117;
dcl-c LDAP_SSL_NOT_AVAILABLE 118;
dcl-c LBER_BOOLEAN 1;
dcl-c LBER_INTEGER 2;
dcl-c LBER_BITSTRING 3;
dcl-c LBER_OCTETSTRING 4;
dcl-c LBER_NULL 5;
dcl-c LBER_ENUMERATED 10;
dcl-c LBER_SEQUENCE 48;
dcl-c LBER_SET 49;
dcl-c LBER_USE_DER 1;
dcl-c LBER_ERROR 4294967295;
dcl-c LDAP_CONTROL_PAGEDRESULTS '1.2.840.113556.1.4.319';
dcl-c LDAP_CONTROL_SORTREQUEST '1.2.840.113556.1.4.473';
dcl-c LDAP_CONTROL_SORTRESPONSE '1.2.840.113556.1.4.474';
dcl-c LDAP_CONTROL_MANAGEDSAIT '2.16.840.1.113730.3.4.2';
dcl-c LDAP_CONTROL_PERSISTENTSEARCH '2.16.840.1.113730.3.4.3';
dcl-c LDAP_CONTROL_ENTRYCHANGE '2.16.840.1.113730.3.4.7';
dcl-c LDAP_CONTROL_PROXYAUTH '2.16.840.1.113730.3.4.18';
dcl-c LDAP_CONTROL_TREEDELETE '1.2.840.113556.1.4.805';
dcl-c LDAP_CONTROL_ASSERTION '1.3.6.1.1.12';
dcl-c LDAP_CONTROL_PWDPOLICY '1.3.6.1.4.1.42.2.27.8.5.1';
dcl-c LDAP_CONTROL_VLVREQUEST '2.16.840.1.113730.3.4.9';
dcl-c LDAP_CONTROL_VLVRESPONSE '2.16.840.1.113730.3.4.10';
dcl-c LDAP_EXOP_START_TLS '1.3.6.1.4.1.1466.20037';
dcl-c LDAP_EXOP_WHO_AM_I '1.3.6.1.4.1.4203.1.11.3';
dcl-c LDAP_EXOP_MODIFY_PASSWD '1.3.6.1.4.1.4203.1.11.1';

dcl-ds berval_t qualified template align;
  bv_len uns(10);
  bv_val pointer;
end-ds;

dcl-ds LDAPControl_t qualified template align;
  ldctl_oid pointer;
  ldctl_value likeds(berval_t);
  ldctl_iscritical char(1);
end-ds;

dcl-ds LDAPMod_t qualified template align;
  mod_op int(10);
  mod_type pointer;
  mod_vals pointer;
end-ds;

dcl-ds LDAPSortKey_t qualified template align;
  attributeType pointer;
  orderingRule pointer;
  reverseOrder int(10);
end-ds;

dcl-ds LDAPURLDesc_t qualified template align;
  lud_host pointer;
  lud_port int(10);
  lud_dn pointer;
  lud_attrs pointer;
  lud_scope int(10);
  lud_filter pointer;
  lud_string pointer;
end-ds;

dcl-ds LDAPVLVInfo_t qualified template align;
  ldvlv_version int(10);
  ldvlv_before_count uns(10);
  ldvlv_after_count uns(10);
  ldvlv_offset uns(10);
  ldvlv_count uns(10);
  ldvlv_attrvalue pointer;
  ldvlv_context pointer;
  ldvlv_extradata pointer;
end-ds;

dcl-ds LDAPVersion_t qualified template;
  sdk_version int(10);
  protocol_version int(10);
  ssl_version int(10);
end-ds;

dcl-ds timeval_t qualified template;
  tv_sec int(10);
  tv_usec int(10);
end-ds;

dcl-pr ldap_init pointer extproc('ldap_init');
  dcl-parm host pointer value options(*string);
  dcl-parm port int(10) value;
end-pr;

dcl-pr ldap_open pointer extproc('ldap_open');
  dcl-parm host pointer value options(*string);
  dcl-parm port int(10) value;
end-pr;

dcl-pr ldap_unbind int(10) extproc('ldap_unbind');
  dcl-parm ld pointer value;
end-pr;

dcl-pr ldap_unbind_s int(10) extproc('ldap_unbind_s');
  dcl-parm ld pointer value;
end-pr;

dcl-pr ldap_unbind_ext int(10) extproc('ldap_unbind_ext');
  dcl-parm ld pointer value;
  dcl-parm sctrls pointer value;
  dcl-parm cctrls pointer value;
end-pr;

dcl-pr ldap_version int(10) extproc('ldap_version');
  dcl-parm ver pointer value;
end-pr;

dcl-pr ldap_set_option int(10) extproc('ldap_set_option');
  dcl-parm ld pointer value;
  dcl-parm option int(10) value;
  dcl-parm invalue pointer value;
end-pr;

dcl-pr ldap_get_option int(10) extproc('ldap_get_option');
  dcl-parm ld pointer value;
  dcl-parm option int(10) value;
  dcl-parm outvalue pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_set_rebind_proc extproc('ldap_set_rebind_proc');
  dcl-parm ld pointer value;
  dcl-parm proc pointer(*proc) value;
end-pr;

dcl-pr ldap_default_dn_set int(10) extproc('ldap_default_dn_set');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
end-pr;

dcl-pr ldap_default_dn_get int(10) extproc('ldap_default_dn_get');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value;
end-pr;

dcl-pr ldap_password_set int(10) extproc('ldap_password_set');
  dcl-parm ld pointer value;
  dcl-parm password pointer value options(*string);
end-pr;

dcl-pr ldap_password_get int(10) extproc('ldap_password_get');
  dcl-parm ld pointer value;
  dcl-parm password pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_proxy_dn_prefix int(10) extproc('ldap_proxy_dn_prefix');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_proxy_dn_prefix_utf8 int(10) extproc('ldap_proxy_dn_prefix_utf8');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
end-pr;

dcl-pr ldap_ssl_init pointer extproc('ldap_ssl_init');
  dcl-parm host pointer value options(*string);
  dcl-parm port int(10) value;
  dcl-parm name pointer value options(*string);
end-pr;

dcl-pr ldap_ssl_client_init int(10) extproc('ldap_ssl_client_init');
  dcl-parm keyring pointer value options(*string);
  dcl-parm keyring_pw pointer value options(*string);
  dcl-parm ssl_timeout int(10) value;
  dcl-parm reason pointer value;
end-pr;

dcl-pr ldap_ssl_environment_init int(10) extproc('ldap_ssl_environment_init');
  dcl-parm keyring pointer value options(*string);
  dcl-parm keyring_pw pointer value options(*string);
  dcl-parm keyring_stash pointer value options(*string);
  dcl-parm ssl_timeout int(10) value;
  dcl-parm reason pointer value;
end-pr;

dcl-pr ldap_ssl_start int(10) extproc('ldap_ssl_start');
  dcl-parm ld pointer value;
  dcl-parm keyring pointer value options(*string);
  dcl-parm keyring_pw pointer value options(*string);
  dcl-parm name pointer value options(*string);
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_ssl_pkcs11_client_init int(10)
       extproc('ldap_ssl_pkcs11_client_init');
  dcl-parm module pointer value options(*string);
  dcl-parm token_label pointer value options(*string);
  dcl-parm token_pw pointer value options(*string);
  dcl-parm ssl_timeout int(10) value;
  dcl-parm reason pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_ssl_pkcs11_environment_init int(10)
       extproc('ldap_ssl_pkcs11_environment_init');
  dcl-parm module pointer value options(*string);
  dcl-parm token_label pointer value options(*string);
  dcl-parm token_pw pointer value options(*string);
  dcl-parm ssl_timeout int(10) value;
  dcl-parm reason pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_ssl_set_fips_mode_np int(10) extproc('ldap_ssl_set_fips_mode_np');
  dcl-parm on int(10) value;
end-pr;

dcl-pr ldap_app_ssl_client_init_np int(10)
       extproc('ldap_app_ssl_client_init_np');
  dcl-parm app_id pointer value options(*string);
  dcl-parm reason pointer value;
end-pr;

dcl-pr ldap_app_ssl_init_np pointer extproc('ldap_app_ssl_init_np');
  dcl-parm host pointer value options(*string);
  dcl-parm port int(10) value;
  dcl-parm app_id pointer value options(*string);
end-pr;

dcl-pr ldap_app_ssl_start_np int(10) extproc('ldap_app_ssl_start_np');
  dcl-parm ld pointer value;
  dcl-parm app_id pointer value options(*string);
end-pr;

dcl-pr ldap_start_tls_s_np int(10) extproc('ldap_start_tls_s_np');
  dcl-parm ld pointer value;
  dcl-parm name pointer value options(*string);
end-pr;

dcl-pr ldap_start_tls_app_np int(10) extproc('ldap_start_tls_app_np');
  dcl-parm ld pointer value;
  dcl-parm app_id pointer value options(*string);
end-pr;

dcl-pr ldap_stop_tls_s_np int(10) extproc('ldap_stop_tls_s_np');
  dcl-parm ld pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_set_cipher int(10) extproc('ldap_set_cipher');
  dcl-parm ld pointer value;
  dcl-parm cipher pointer value options(*string);
end-pr;

dcl-pr ldap_simple_bind int(10) extproc('ldap_simple_bind');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
  dcl-parm passwd pointer value options(*string);
end-pr;

dcl-pr ldap_simple_bind_s int(10) extproc('ldap_simple_bind_s');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
  dcl-parm passwd pointer value options(*string);
end-pr;

dcl-pr ldap_bind int(10) extproc('ldap_bind');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
  dcl-parm cred pointer value options(*string);
  dcl-parm method int(10) value;
end-pr;

dcl-pr ldap_bind_s int(10) extproc('ldap_bind_s');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
  dcl-parm cred pointer value options(*string);
  dcl-parm method int(10) value;
end-pr;

dcl-pr ldap_sasl_bind int(10) extproc('ldap_sasl_bind');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
  dcl-parm mechanism pointer value options(*string);
  dcl-parm cred pointer value;
  dcl-parm sctrls pointer value;
  dcl-parm cctrls pointer value;
  dcl-parm msgidp pointer value;
end-pr;

dcl-pr ldap_sasl_bind_s int(10) extproc('ldap_sasl_bind_s');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
  dcl-parm mechanism pointer value options(*string);
  dcl-parm cred pointer value;
  dcl-parm sctrls pointer value;
  dcl-parm cctrls pointer value;
  dcl-parm servercredp pointer value;
end-pr;

dcl-pr ldap_parse_sasl_bind_result int(10)
       extproc('ldap_parse_sasl_bind_result');
  dcl-parm ld pointer value;
  dcl-parm res pointer value;
  dcl-parm servercredp pointer value;
  dcl-parm freeit int(10) value;
end-pr;

dcl-pr ldap_get_bind_controls int(10) extproc('ldap_get_bind_controls');
  dcl-parm ld pointer value;
  dcl-parm ctrls pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_plugin_sasl_bind_s int(10) extproc('ldap_plugin_sasl_bind_s');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
  dcl-parm mechanism pointer value options(*string);
  dcl-parm cred pointer value;
  dcl-parm sctrls pointer value;
  dcl-parm cctrls pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_krb_init_tkt int(10) extproc('ldap_krb_init_tkt');
  dcl-parm name pointer value options(*string);
  dcl-parm password pointer value options(*string);
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_krb_cc_destroy int(10) extproc('ldap_krb_cc_destroy');
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_krb_ctx_destroy int(10) extproc('ldap_krb_ctx_destroy');
end-pr;

dcl-pr ldap_search int(10) extproc('ldap_search');
  dcl-parm ld pointer value;
  dcl-parm base pointer value options(*string);
  dcl-parm scope int(10) value;
  dcl-parm filter pointer value options(*string);
  dcl-parm attrs pointer value;
  dcl-parm attrsonly int(10) value;
end-pr;

dcl-pr ldap_search_s int(10) extproc('ldap_search_s');
  dcl-parm ld pointer value;
  dcl-parm base pointer value options(*string);
  dcl-parm scope int(10) value;
  dcl-parm filter pointer value options(*string);
  dcl-parm attrs pointer value;
  dcl-parm attrsonly int(10) value;
  dcl-parm res pointer value;
end-pr;

dcl-pr ldap_search_st int(10) extproc('ldap_search_st');
  dcl-parm ld pointer value;
  dcl-parm base pointer value options(*string);
  dcl-parm scope int(10) value;
  dcl-parm filter pointer value options(*string);
  dcl-parm attrs pointer value;
  dcl-parm attrsonly int(10) value;
  dcl-parm timeout pointer value;
  dcl-parm res pointer value;
end-pr;

dcl-pr ldap_search_ext int(10) extproc('ldap_search_ext');
  dcl-parm ld pointer value;
  dcl-parm base pointer value options(*string);
  dcl-parm scope int(10) value;
  dcl-parm filter pointer value options(*string);
  dcl-parm attrs pointer value;
  dcl-parm attrsonly int(10) value;
  dcl-parm sctrls pointer value;
  dcl-parm cctrls pointer value;
  dcl-parm timeout pointer value;
  dcl-parm sizelimit int(10) value;
  dcl-parm msgidp pointer value;
end-pr;

dcl-pr ldap_search_ext_s int(10) extproc('ldap_search_ext_s');
  dcl-parm ld pointer value;
  dcl-parm base pointer value options(*string);
  dcl-parm scope int(10) value;
  dcl-parm filter pointer value options(*string);
  dcl-parm attrs pointer value;
  dcl-parm attrsonly int(10) value;
  dcl-parm sctrls pointer value;
  dcl-parm cctrls pointer value;
  dcl-parm timeout pointer value;
  dcl-parm sizelimit int(10) value;
  dcl-parm res pointer value;
end-pr;

dcl-pr ldap_url_search int(10) extproc('ldap_url_search');
  dcl-parm ld pointer value;
  dcl-parm url pointer value options(*string);
  dcl-parm attrsonly int(10) value;
end-pr;

dcl-pr ldap_url_search_s int(10) extproc('ldap_url_search_s');
  dcl-parm ld pointer value;
  dcl-parm url pointer value options(*string);
  dcl-parm attrsonly int(10) value;
  dcl-parm res pointer value;
end-pr;

dcl-pr ldap_url_search_st int(10) extproc('ldap_url_search_st');
  dcl-parm ld pointer value;
  dcl-parm url pointer value options(*string);
  dcl-parm attrsonly int(10) value;
  dcl-parm timeout pointer value;
  dcl-parm res pointer value;
end-pr;

dcl-pr ldap_result int(10) extproc('ldap_result');
  dcl-parm ld pointer value;
  dcl-parm msgid int(10) value;
  dcl-parm all int(10) value;
  dcl-parm timeout pointer value;
  dcl-parm result pointer value;
end-pr;

dcl-pr ldap_msgfree int(10) extproc('ldap_msgfree');
  dcl-parm lm pointer value;
end-pr;

dcl-pr ldap_msgtype int(10) extproc('ldap_msgtype');
  dcl-parm lm pointer value;
end-pr;

dcl-pr ldap_msgid int(10) extproc('ldap_msgid');
  dcl-parm lm pointer value;
end-pr;

dcl-pr ldap_result2error int(10) extproc('ldap_result2error');
  dcl-parm ld pointer value;
  dcl-parm res pointer value;
  dcl-parm freeit int(10) value;
end-pr;

dcl-pr ldap_parse_result int(10) extproc('ldap_parse_result');
  dcl-parm ld pointer value;
  dcl-parm res pointer value;
  dcl-parm errcodep pointer value;
  dcl-parm matcheddnp pointer value;
  dcl-parm errmsgp pointer value;
  dcl-parm referralsp pointer value;
  dcl-parm sctrlsp pointer value;
  dcl-parm freeit int(10) value;
end-pr;

dcl-pr ldap_parse_extended_result int(10) extproc('ldap_parse_extended_result');
  dcl-parm ld pointer value;
  dcl-parm res pointer value;
  dcl-parm resultoidp pointer value;
  dcl-parm resultdatap pointer value;
  dcl-parm freeit int(10) value;
end-pr;

dcl-pr ldap_parse_extended_result_w_controls int(10)
       extproc('ldap_parse_extended_result_w_controls');
  dcl-parm ld pointer value;
  dcl-parm res pointer value;
  dcl-parm resultoidp pointer value;
  dcl-parm resultdatap pointer value;
  dcl-parm sctrlsp pointer value;
  dcl-parm freeit int(10) value;
end-pr;

dcl-pr ldap_parse_reference int(10) extproc('ldap_parse_reference');
  dcl-parm ld pointer value;
  dcl-parm ref pointer value;
  dcl-parm referralsp pointer value;
  dcl-parm sctrlsp pointer value;
  dcl-parm freeit int(10) value;
end-pr;

dcl-pr ldap_parse_reference_np int(10) extproc('ldap_parse_reference_np');
  dcl-parm ld pointer value;
  dcl-parm ref pointer value;
  dcl-parm referralsp pointer value;
  dcl-parm sctrlsp pointer value;
  dcl-parm freeit int(10) value;
end-pr;

dcl-pr ldap_first_message pointer extproc('ldap_first_message');
  dcl-parm ld pointer value;
  dcl-parm res pointer value;
end-pr;

dcl-pr ldap_next_message pointer extproc('ldap_next_message');
  dcl-parm ld pointer value;
  dcl-parm msg pointer value;
end-pr;

dcl-pr ldap_count_messages int(10) extproc('ldap_count_messages');
  dcl-parm ld pointer value;
  dcl-parm res pointer value;
end-pr;

dcl-pr ldap_first_entry pointer extproc('ldap_first_entry');
  dcl-parm ld pointer value;
  dcl-parm res pointer value;
end-pr;

dcl-pr ldap_next_entry pointer extproc('ldap_next_entry');
  dcl-parm ld pointer value;
  dcl-parm entry pointer value;
end-pr;

dcl-pr ldap_count_entries int(10) extproc('ldap_count_entries');
  dcl-parm ld pointer value;
  dcl-parm res pointer value;
end-pr;

dcl-pr ldap_first_reference pointer extproc('ldap_first_reference');
  dcl-parm ld pointer value;
  dcl-parm res pointer value;
end-pr;

dcl-pr ldap_next_reference pointer extproc('ldap_next_reference');
  dcl-parm ld pointer value;
  dcl-parm ref pointer value;
end-pr;

dcl-pr ldap_count_references int(10) extproc('ldap_count_references');
  dcl-parm ld pointer value;
  dcl-parm res pointer value;
end-pr;

dcl-pr ldap_get_dn pointer extproc('ldap_get_dn');
  dcl-parm ld pointer value;
  dcl-parm entry pointer value;
end-pr;

dcl-pr ldap_first_attribute pointer extproc('ldap_first_attribute');
  dcl-parm ld pointer value;
  dcl-parm entry pointer value;
  dcl-parm ber pointer value;
end-pr;

dcl-pr ldap_next_attribute pointer extproc('ldap_next_attribute');
  dcl-parm ld pointer value;
  dcl-parm entry pointer value;
  dcl-parm ber pointer value;
end-pr;

dcl-pr ldap_count_attributes int(10) extproc('ldap_count_attributes');
  dcl-parm ld pointer value;
  dcl-parm entry pointer value;
end-pr;

dcl-pr ldap_get_values pointer extproc('ldap_get_values');
  dcl-parm ld pointer value;
  dcl-parm entry pointer value;
  dcl-parm attr pointer value options(*string);
end-pr;

dcl-pr ldap_get_values_len pointer extproc('ldap_get_values_len');
  dcl-parm ld pointer value;
  dcl-parm entry pointer value;
  dcl-parm attr pointer value options(*string);
end-pr;

dcl-pr ldap_count_values int(10) extproc('ldap_count_values');
  dcl-parm vals pointer value;
end-pr;

dcl-pr ldap_count_values_len int(10) extproc('ldap_count_values_len');
  dcl-parm vals pointer value;
end-pr;

dcl-pr ldap_value_free extproc('ldap_value_free');
  dcl-parm vals pointer value;
end-pr;

dcl-pr ldap_value_free_len extproc('ldap_value_free_len');
  dcl-parm vals pointer value;
end-pr;

dcl-pr ldap_get_entry_controls int(10) extproc('ldap_get_entry_controls');
  dcl-parm ld pointer value;
  dcl-parm entry pointer value;
  dcl-parm sctrlsp pointer value;
end-pr;

dcl-pr ldap_get_entry_controls_np int(10) extproc('ldap_get_entry_controls_np');
  dcl-parm ld pointer value;
  dcl-parm entry pointer value;
  dcl-parm sctrlsp pointer value;
end-pr;

dcl-pr ldap_memfree extproc('ldap_memfree');
  dcl-parm mem pointer value options(*string);
end-pr;

dcl-pr ldap_ber_free extproc('ldap_ber_free');
  dcl-parm ber pointer value;
  dcl-parm freebuf int(10) value;
end-pr;

dcl-pr ldap_mods_free extproc('ldap_mods_free');
  dcl-parm mods pointer value;
  dcl-parm freemods int(10) value;
end-pr;

dcl-pr ldap_explode_dn pointer extproc('ldap_explode_dn');
  dcl-parm dn pointer value options(*string);
  dcl-parm notypes int(10) value;
end-pr;

dcl-pr ldap_explode_dn_utf8 pointer extproc('ldap_explode_dn_utf8');
  dcl-parm dn pointer value options(*string);
  dcl-parm notypes int(10) value;
end-pr;

dcl-pr ldap_explode_rdn pointer extproc('ldap_explode_rdn');
  dcl-parm rdn pointer value options(*string);
  dcl-parm notypes int(10) value;
end-pr;

dcl-pr ldap_explode_rdn_utf8 pointer extproc('ldap_explode_rdn_utf8');
  dcl-parm rdn pointer value options(*string);
  dcl-parm notypes int(10) value;
end-pr;

dcl-pr ldap_explode_dns pointer extproc('ldap_explode_dns');
  dcl-parm dns pointer value options(*string);
end-pr;

dcl-pr ldap_dn2ufn pointer extproc('ldap_dn2ufn');
  dcl-parm dn pointer value options(*string);
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_friendly_name pointer extproc('ldap_friendly_name');
  dcl-parm filename pointer value options(*string);
  dcl-parm name pointer value options(*string);
  dcl-parm map pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_free_friendlymap extproc('ldap_free_friendlymap');
  dcl-parm map pointer value;
end-pr;

dcl-pr ldap_add int(10) extproc('ldap_add');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
  dcl-parm attrs pointer value;
end-pr;

dcl-pr ldap_add_s int(10) extproc('ldap_add_s');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
  dcl-parm attrs pointer value;
end-pr;

dcl-pr ldap_add_ext int(10) extproc('ldap_add_ext');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
  dcl-parm attrs pointer value;
  dcl-parm sctrls pointer value;
  dcl-parm cctrls pointer value;
  dcl-parm msgidp pointer value;
end-pr;

dcl-pr ldap_add_ext_s int(10) extproc('ldap_add_ext_s');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
  dcl-parm attrs pointer value;
  dcl-parm sctrls pointer value;
  dcl-parm cctrls pointer value;
end-pr;

dcl-pr ldap_modify int(10) extproc('ldap_modify');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
  dcl-parm mods pointer value;
end-pr;

dcl-pr ldap_modify_s int(10) extproc('ldap_modify_s');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
  dcl-parm mods pointer value;
end-pr;

dcl-pr ldap_modify_ext int(10) extproc('ldap_modify_ext');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
  dcl-parm mods pointer value;
  dcl-parm sctrls pointer value;
  dcl-parm cctrls pointer value;
  dcl-parm msgidp pointer value;
end-pr;

dcl-pr ldap_modify_ext_s int(10) extproc('ldap_modify_ext_s');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
  dcl-parm mods pointer value;
  dcl-parm sctrls pointer value;
  dcl-parm cctrls pointer value;
end-pr;

dcl-pr ldap_delete int(10) extproc('ldap_delete');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
end-pr;

dcl-pr ldap_delete_s int(10) extproc('ldap_delete_s');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
end-pr;

dcl-pr ldap_delete_ext int(10) extproc('ldap_delete_ext');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
  dcl-parm sctrls pointer value;
  dcl-parm cctrls pointer value;
  dcl-parm msgidp pointer value;
end-pr;

dcl-pr ldap_delete_ext_s int(10) extproc('ldap_delete_ext_s');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
  dcl-parm sctrls pointer value;
  dcl-parm cctrls pointer value;
end-pr;

dcl-pr ldap_modrdn int(10) extproc('ldap_modrdn');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
  dcl-parm newrdn pointer value options(*string);
  dcl-parm deleteoldrdn int(10) value;
end-pr;

dcl-pr ldap_modrdn_s int(10) extproc('ldap_modrdn_s');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
  dcl-parm newrdn pointer value options(*string);
  dcl-parm deleteoldrdn int(10) value;
end-pr;

dcl-pr ldap_rename int(10) extproc('ldap_rename');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
  dcl-parm newrdn pointer value options(*string);
  dcl-parm newparent pointer value options(*string);
  dcl-parm deleteoldrdn int(10) value;
  dcl-parm sctrls pointer value;
  dcl-parm cctrls pointer value;
  dcl-parm msgidp pointer value;
end-pr;

dcl-pr ldap_rename_s int(10) extproc('ldap_rename_s');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
  dcl-parm newrdn pointer value options(*string);
  dcl-parm newparent pointer value options(*string);
  dcl-parm deleteoldrdn int(10) value;
  dcl-parm sctrls pointer value;
  dcl-parm cctrls pointer value;
end-pr;

dcl-pr ldap_compare int(10) extproc('ldap_compare');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
  dcl-parm attr pointer value options(*string);
  dcl-parm value pointer value options(*string);
end-pr;

dcl-pr ldap_compare_s int(10) extproc('ldap_compare_s');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
  dcl-parm attr pointer value options(*string);
  dcl-parm value pointer value options(*string);
end-pr;

dcl-pr ldap_compare_ext int(10) extproc('ldap_compare_ext');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
  dcl-parm attr pointer value options(*string);
  dcl-parm bvalue pointer value;
  dcl-parm sctrls pointer value;
  dcl-parm cctrls pointer value;
  dcl-parm msgidp pointer value;
end-pr;

dcl-pr ldap_compare_ext_s int(10) extproc('ldap_compare_ext_s');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
  dcl-parm attr pointer value options(*string);
  dcl-parm bvalue pointer value;
  dcl-parm sctrls pointer value;
  dcl-parm cctrls pointer value;
end-pr;

dcl-pr ldap_abandon int(10) extproc('ldap_abandon');
  dcl-parm ld pointer value;
  dcl-parm msgid int(10) value;
end-pr;

dcl-pr ldap_abandon_ext int(10) extproc('ldap_abandon_ext');
  dcl-parm ld pointer value;
  dcl-parm msgid int(10) value;
  dcl-parm sctrls pointer value;
  dcl-parm cctrls pointer value;
end-pr;

dcl-pr ldap_extended_operation int(10) extproc('ldap_extended_operation');
  dcl-parm ld pointer value;
  dcl-parm reqoid pointer value options(*string);
  dcl-parm reqdata pointer value;
  dcl-parm sctrls pointer value;
  dcl-parm cctrls pointer value;
  dcl-parm msgidp pointer value;
end-pr;

dcl-pr ldap_extended_operation_s int(10) extproc('ldap_extended_operation_s');
  dcl-parm ld pointer value;
  dcl-parm reqoid pointer value options(*string);
  dcl-parm reqdata pointer value;
  dcl-parm sctrls pointer value;
  dcl-parm cctrls pointer value;
  dcl-parm retoidp pointer value;
  dcl-parm retdatap pointer value;
end-pr;

dcl-pr ldap_create_control int(10) extproc('ldap_create_control');
  dcl-parm oid pointer value options(*string);
  dcl-parm ber pointer value;
  dcl-parm iscritical int(10) value;
  dcl-parm ctrlp pointer value;
end-pr;

dcl-pr ldap_create_page_control int(10) extproc('ldap_create_page_control');
  dcl-parm ld pointer value;
  dcl-parm pagesize uns(10) value;
  dcl-parm cookie pointer value;
  dcl-parm iscritical int(3) value;
  dcl-parm output pointer value;
end-pr;

dcl-pr ldap_parse_page_control int(10) extproc('ldap_parse_page_control');
  dcl-parm ld pointer value;
  dcl-parm ctrls pointer value;
  dcl-parm totalcount pointer value;
  dcl-parm cookie pointer value;
end-pr;

dcl-pr ldap_create_sort_keylist int(10) extproc('ldap_create_sort_keylist');
  dcl-parm sortkeylist pointer value;
  dcl-parm keystring pointer value options(*string);
end-pr;

dcl-pr ldap_free_sort_keylist extproc('ldap_free_sort_keylist');
  dcl-parm sortkeylist pointer value;
end-pr;

dcl-pr ldap_create_sort_control int(10) extproc('ldap_create_sort_control');
  dcl-parm ld pointer value;
  dcl-parm keylist pointer value;
  dcl-parm iscritical int(3) value;
  dcl-parm output pointer value;
end-pr;

dcl-pr ldap_parse_sort_control int(10) extproc('ldap_parse_sort_control');
  dcl-parm ld pointer value;
  dcl-parm ctrls pointer value;
  dcl-parm returncode pointer value;
  dcl-parm attribute pointer value;
end-pr;

dcl-pr ldap_create_proxyauth_control int(10)
       extproc('ldap_create_proxyauth_control');
  dcl-parm ld pointer value;
  dcl-parm proxydn pointer value options(*string);
  dcl-parm iscritical int(3) value;
  dcl-parm ctrlp pointer value;
end-pr;

dcl-pr ldap_create_persistentsearch_control int(10)
       extproc('ldap_create_persistentsearch_control');
  dcl-parm ld pointer value;
  dcl-parm changetypes int(10) value;
  dcl-parm changesonly int(10) value;
  dcl-parm returnecs int(10) value;
  dcl-parm iscritical int(3) value;
  dcl-parm ctrlp pointer value;
end-pr;

dcl-pr ldap_parse_entrychange_control int(10)
       extproc('ldap_parse_entrychange_control');
  dcl-parm ld pointer value;
  dcl-parm ctrls pointer value;
  dcl-parm changetype pointer value;
  dcl-parm prevdn pointer value;
  dcl-parm haschangenumber pointer value;
  dcl-parm changenumber pointer value;
end-pr;

dcl-pr ldap_create_assertion_control int(10)
       extproc('ldap_create_assertion_control');
  dcl-parm ld pointer value;
  dcl-parm filter pointer value options(*string);
  dcl-parm iscritical int(3) value;
  dcl-parm ctrlp pointer value;
end-pr;

dcl-pr ldap_create_vlv_control int(10) extproc('ldap_create_vlv_control');
  dcl-parm ld pointer value;
  dcl-parm vlvinfo pointer value;
  dcl-parm ctrlp pointer value;
end-pr;

dcl-pr ldap_parse_vlv_control int(10) extproc('ldap_parse_vlv_control');
  dcl-parm ld pointer value;
  dcl-parm ctrls pointer value;
  dcl-parm targetpos pointer value;
  dcl-parm listcount pointer value;
  dcl-parm context pointer value;
  dcl-parm errcode pointer value;
end-pr;

dcl-pr ldap_parse_pwdpolicy_response int(10)
       extproc('ldap_parse_pwdpolicy_response');
  dcl-parm ctrls pointer value;
  dcl-parm warning pointer value;
  dcl-parm value pointer value;
  dcl-parm error pointer value;
end-pr;

dcl-pr ldap_pwdpolicy_err2string pointer extproc('ldap_pwdpolicy_err2string');
  dcl-parm error int(10) value;
end-pr;

dcl-pr ldap_control_free extproc('ldap_control_free');
  dcl-parm ctrl pointer value;
end-pr;

dcl-pr ldap_controls_free extproc('ldap_controls_free');
  dcl-parm ctrls pointer value;
end-pr;

dcl-pr ldap_copy_controls int(10) extproc('ldap_copy_controls');
  dcl-parm to pointer value;
  dcl-parm from pointer value;
end-pr;

dcl-pr ldap_insert_control int(10) extproc('ldap_insert_control');
  dcl-parm newcontrol pointer value;
  dcl-parm ctrllist pointer value;
end-pr;

dcl-pr ldap_add_control int(10) extproc('ldap_add_control');
  dcl-parm oid pointer value options(*string);
  dcl-parm len uns(10) value;
  dcl-parm value pointer value options(*string);
  dcl-parm iscritical int(10) value;
  dcl-parm ctrllist pointer value;
end-pr;

dcl-pr ldap_remove_control int(10) extproc('ldap_remove_control');
  dcl-parm delcontrol pointer value;
  dcl-parm ctrllist pointer value;
  dcl-parm freeit int(10) value;
end-pr;

dcl-pr ldap_merge_control_lists int(10) extproc('ldap_merge_control_lists');
  dcl-parm list1 pointer value;
  dcl-parm list2 pointer value;
  dcl-parm merged pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_create_group_control int(10) extproc('ldap_create_group_control');
  dcl-parm ld pointer value;
  dcl-parm groups pointer value;
  dcl-parm iscritical int(3) value;
  dcl-parm ctrlp pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_create_group_control_from_list int(10)
       extproc('ldap_create_group_control_from_list');
  dcl-parm ld pointer value;
  dcl-parm groups pointer value options(*string);
  dcl-parm iscritical int(3) value;
  dcl-parm ctrlp pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_create_modify_groups_only_control int(10)
       extproc('ldap_create_modify_groups_only_control');
  dcl-parm ld pointer value;
  dcl-parm iscritical int(3) value;
  dcl-parm ctrlp pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_create_omit_group_referential_integrity_control int(10)
       extproc('ldap_create_omit_group_referential_integrity_control');
  dcl-parm ld pointer value;
  dcl-parm iscritical int(3) value;
  dcl-parm ctrlp pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_create_limit_num_values_control int(10)
       extproc('ldap_create_limit_num_values_control');
  dcl-parm ld pointer value;
  dcl-parm maxvalues int(10) value;
  dcl-parm maxpertype int(10) value;
  dcl-parm iscritical int(3) value;
  dcl-parm ctrlp pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_parse_limit_num_values_response int(10)
       extproc('ldap_parse_limit_num_values_response');
  dcl-parm ld pointer value;
  dcl-parm ctrls pointer value;
  dcl-parm response pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_free_limit_num_values_response
       extproc('ldap_free_limit_num_values_response');
  dcl-parm response pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_create_chaining_audit_control int(10)
       extproc('ldap_create_chaining_audit_control');
  dcl-parm ld pointer value;
  dcl-parm data pointer value options(*string);
  dcl-parm iscritical int(3) value;
  dcl-parm ctrlp pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_create_return_deleted_objects_control int(10)
       extproc('ldap_create_return_deleted_objects_control');
  dcl-parm ld pointer value;
  dcl-parm iscritical int(3) value;
  dcl-parm ctrlp pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_create_transaction_control int(10)
       extproc('ldap_create_transaction_control');
  dcl-parm ld pointer value;
  dcl-parm tranid pointer value options(*string);
  dcl-parm iscritical int(3) value;
  dcl-parm ctrlp pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_create_account_status_request int(10)
       extproc('ldap_create_account_status_request');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
  dcl-parm request pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_create_effective_pwdpolicy_request int(10)
       extproc('ldap_create_effective_pwdpolicy_request');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
  dcl-parm request pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_parse_effective_pwdpolicy_response int(10)
       extproc('ldap_parse_effective_pwdpolicy_response');
  dcl-parm ld pointer value;
  dcl-parm response pointer value;
  dcl-parm policy pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_create_get_file_request int(10)
       extproc('ldap_create_get_file_request');
  dcl-parm ld pointer value;
  dcl-parm name pointer value options(*string);
  dcl-parm request pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_create_group_eval_request int(10)
       extproc('ldap_create_group_eval_request');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
  dcl-parm attrs pointer value;
  dcl-parm request pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_create_locate_entry_request int(10)
       extproc('ldap_create_locate_entry_request');
  dcl-parm ld pointer value;
  dcl-parm dns pointer value;
  dcl-parm request pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_create_online_backup_request int(10)
       extproc('ldap_create_online_backup_request');
  dcl-parm ld pointer value;
  dcl-parm path pointer value options(*string);
  dcl-parm request pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_create_resume_role_request int(10)
       extproc('ldap_create_resume_role_request');
  dcl-parm ld pointer value;
  dcl-parm role pointer value options(*string);
  dcl-parm request pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_create_user_type_request int(10)
       extproc('ldap_create_user_type_request');
  dcl-parm ld pointer value;
  dcl-parm request pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_create_password_policy_bind_init_request int(10)
       extproc('ldap_create_password_policy_bind_init_request');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
  dcl-parm request pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_create_password_policy_bind_finalize_request int(10)
       extproc('ldap_create_password_policy_bind_finalize_request');
  dcl-parm ld pointer value;
  dcl-parm dn pointer value options(*string);
  dcl-parm result int(10) value;
  dcl-parm request pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_create_abort_transaction_request int(10)
       extproc('ldap_create_abort_transaction_request');
  dcl-parm ld pointer value;
  dcl-parm tranid pointer value options(*string);
  dcl-parm request pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_create_commit_transaction_request int(10)
       extproc('ldap_create_commit_transaction_request');
  dcl-parm ld pointer value;
  dcl-parm tranid pointer value options(*string);
  dcl-parm request pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_create_prepare_transaction_request int(10)
       extproc('ldap_create_prepare_transaction_request');
  dcl-parm ld pointer value;
  dcl-parm tranid pointer value options(*string);
  dcl-parm request pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_start_transaction int(10) extproc('ldap_start_transaction');
  dcl-parm ld pointer value;
  dcl-parm sctrls pointer value;
  dcl-parm cctrls pointer value;
  dcl-parm msgidp pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_start_transaction_s int(10) extproc('ldap_start_transaction_s');
  dcl-parm ld pointer value;
  dcl-parm sctrls pointer value;
  dcl-parm cctrls pointer value;
  dcl-parm tranid pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_end_transaction int(10) extproc('ldap_end_transaction');
  dcl-parm ld pointer value;
  dcl-parm tranid pointer value options(*string);
  dcl-parm commit int(10) value;
  dcl-parm sctrls pointer value;
  dcl-parm cctrls pointer value;
  dcl-parm msgidp pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_end_transaction_s int(10) extproc('ldap_end_transaction_s');
  dcl-parm ld pointer value;
  dcl-parm tranid pointer value options(*string);
  dcl-parm commit int(10) value;
  dcl-parm sctrls pointer value;
  dcl-parm cctrls pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_prepare_transaction int(10) extproc('ldap_prepare_transaction');
  dcl-parm ld pointer value;
  dcl-parm tranid pointer value options(*string);
  dcl-parm sctrls pointer value;
  dcl-parm cctrls pointer value;
  dcl-parm msgidp pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_prepare_transaction_s int(10) extproc('ldap_prepare_transaction_s');
  dcl-parm ld pointer value;
  dcl-parm tranid pointer value options(*string);
  dcl-parm sctrls pointer value;
  dcl-parm cctrls pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_get_tran_id int(10) extproc('ldap_get_tran_id');
  dcl-parm ld pointer value;
  dcl-parm res pointer value;
  dcl-parm tranid pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_server_locate int(10) extproc('ldap_server_locate');
  dcl-parm request pointer value;
  dcl-parm hosts pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_server_free_list extproc('ldap_server_free_list');
  dcl-parm hosts pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_server_conf_save int(10) extproc('ldap_server_conf_save');
  dcl-parm filename pointer value options(*string);
  dcl-parm ttl uns(10) value;
  dcl-parm hosts pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_enetwork_domain_set int(10) extproc('ldap_enetwork_domain_set');
  dcl-parm domain pointer value options(*string);
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_enetwork_domain_get int(10) extproc('ldap_enetwork_domain_get');
  dcl-parm domain pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_plugin_pblock_get int(10) extproc('ldap_plugin_pblock_get');
  dcl-parm pb pointer value;
  dcl-parm arg int(10) value;
  dcl-parm value pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_plugin_pblock_set int(10) extproc('ldap_plugin_pblock_set');
  dcl-parm pb pointer value;
  dcl-parm arg int(10) value;
  dcl-parm value pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_plugin_pblock_free extproc('ldap_plugin_pblock_free');
  dcl-parm pb pointer value;
end-pr;

dcl-pr ldap_err2string pointer extproc('ldap_err2string');
  dcl-parm err int(10) value;
end-pr;

dcl-pr ldap_perror extproc('ldap_perror');
  dcl-parm ld pointer value;
  dcl-parm s pointer value options(*string);
end-pr;

dcl-pr ldap_get_lderrno int(10) extproc('ldap_get_lderrno');
  dcl-parm ld pointer value;
  dcl-parm m pointer value;
  dcl-parm s pointer value;
end-pr;

dcl-pr ldap_set_lderrno int(10) extproc('ldap_set_lderrno');
  dcl-parm ld pointer value;
  dcl-parm e int(10) value;
  dcl-parm m pointer value options(*string);
  dcl-parm s pointer value options(*string);
end-pr;

dcl-pr ldap_get_errno int(10) extproc('ldap_get_errno');
  dcl-parm ld pointer value;
end-pr;

dcl-pr ldap_get_exterror int(10) extproc('ldap_get_exterror');
  dcl-parm ld pointer value;
end-pr;

dcl-pr ldap_is_ldap_url int(10) extproc('ldap_is_ldap_url');
  dcl-parm url pointer value options(*string);
end-pr;

dcl-pr ldap_is_ldap_url_utf8 int(10) extproc('ldap_is_ldap_url_utf8');
  dcl-parm url pointer value options(*string);
end-pr;

dcl-pr ldap_url_parse int(10) extproc('ldap_url_parse');
  dcl-parm url pointer value options(*string);
  dcl-parm ludpp pointer value;
end-pr;

dcl-pr ldap_url_parse_utf8 int(10) extproc('ldap_url_parse_utf8');
  dcl-parm url pointer value options(*string);
  dcl-parm ludpp pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_url_parse_internal int(10) extproc('ldap_url_parse_internal');
  dcl-parm url pointer value options(*string);
  dcl-parm ludpp pointer value;
end-pr;

dcl-pr ldap_free_urldesc extproc('ldap_free_urldesc');
  dcl-parm ludp pointer value;
end-pr;

dcl-pr ldap_set_iconv_local_codepage int(10)
       extproc('ldap_set_iconv_local_codepage');
  dcl-parm codepage int(10) value;
end-pr;

dcl-pr ldap_set_iconv_local_charset int(10)
       extproc('ldap_set_iconv_local_charset');
  dcl-parm charset pointer value options(*string);
end-pr;

dcl-pr ldap_get_iconv_local_codepage int(10)
       extproc('ldap_get_iconv_local_codepage');
end-pr;

dcl-pr ldap_xlate_local_to_utf8 int(10) extproc('ldap_xlate_local_to_utf8');
  dcl-parm inbuf pointer value options(*string);
  dcl-parm inlen pointer value;
  dcl-parm outbuf pointer value options(*string);
  dcl-parm outlen pointer value;
end-pr;

dcl-pr ldap_xlate_utf8_to_local int(10) extproc('ldap_xlate_utf8_to_local');
  dcl-parm inbuf pointer value options(*string);
  dcl-parm inlen pointer value;
  dcl-parm outbuf pointer value options(*string);
  dcl-parm outlen pointer value;
end-pr;

dcl-pr ldap_xlate_local_to_unicode int(10)
       extproc('ldap_xlate_local_to_unicode');
  dcl-parm inbuf pointer value options(*string);
  dcl-parm inlen pointer value;
  dcl-parm outbuf pointer value options(*string);
  dcl-parm outlen pointer value;
end-pr;

dcl-pr ldap_xlate_unicode_to_local int(10)
       extproc('ldap_xlate_unicode_to_local');
  dcl-parm inbuf pointer value options(*string);
  dcl-parm inlen pointer value;
  dcl-parm outbuf pointer value options(*string);
  dcl-parm outlen pointer value;
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_xlate_local_to_utf8_wrp int(10)
       extproc('ldap_xlate_local_to_utf8_wrp');
  dcl-parm inbuf pointer value options(*string);
  dcl-parm inlen pointer value;
  dcl-parm outbuf pointer value options(*string);
  dcl-parm outlen pointer value;
end-pr;

dcl-pr ldap_char_size int(10) extproc('ldap_char_size');
  dcl-parm p pointer value options(*string);
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_set_locale int(10) extproc('ldap_set_locale');
  dcl-parm locale pointer value options(*string);
end-pr;

// Answers LDAP_NOT_SUPPORTED on QS/400.
dcl-pr ldap_get_locale pointer extproc('ldap_get_locale');
end-pr;

dcl-pr ber_alloc_t pointer extproc('ber_alloc_t');
  dcl-parm options int(10) value;
end-pr;

dcl-pr ber_init pointer extproc('ber_init');
  dcl-parm bv pointer value;
end-pr;

dcl-pr ber_init2 extproc('ber_init2');
  dcl-parm ber pointer value;
  dcl-parm bv pointer value;
  dcl-parm options int(10) value;
end-pr;

dcl-pr ber_flatten int(10) extproc('ber_flatten');
  dcl-parm ber pointer value;
  dcl-parm bvptr pointer value;
end-pr;

dcl-pr ber_free extproc('ber_free');
  dcl-parm ber pointer value;
  dcl-parm freebuf int(10) value;
end-pr;

dcl-pr ber_peek_tag uns(10) extproc('ber_peek_tag');
  dcl-parm ber pointer value;
  dcl-parm len pointer value;
end-pr;

dcl-pr ber_skip_tag uns(10) extproc('ber_skip_tag');
  dcl-parm ber pointer value;
  dcl-parm len pointer value;
end-pr;

dcl-pr ber_get_tag uns(10) extproc('ber_get_tag');
  dcl-parm ber pointer value;
end-pr;

dcl-pr ber_skip_element uns(10) extproc('ber_skip_element');
  dcl-parm ber pointer value;
  dcl-parm bv pointer value;
end-pr;

dcl-pr ber_first_element uns(10) extproc('ber_first_element');
  dcl-parm ber pointer value;
  dcl-parm len pointer value;
  dcl-parm last pointer value;
end-pr;

dcl-pr ber_next_element uns(10) extproc('ber_next_element');
  dcl-parm ber pointer value;
  dcl-parm len pointer value;
  dcl-parm last pointer value options(*string);
end-pr;

dcl-pr ber_bvdup pointer extproc('ber_bvdup');
  dcl-parm bv pointer value;
end-pr;

dcl-pr ber_bvfree extproc('ber_bvfree');
  dcl-parm bv pointer value;
end-pr;

dcl-pr ber_bvecfree extproc('ber_bvecfree');
  dcl-parm bv pointer value;
end-pr;
