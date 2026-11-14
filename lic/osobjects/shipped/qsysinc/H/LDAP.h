/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work */
/* is licensed under the terms of the MIT license which can be found in */
/* the root directory of this project.                                  */
/*                                                                      */
/* LDAP: QS/400's LDAP client interface (service program QGLDCLNT).     */
/* Generated from runtime/ldapclnt; see docs/DIRSRV.md section 13.      */
/* Strings are in the job's CCSID unless LDAP_OPT_UTF8_IO is on. Where  */
/* the LDAP C API draft gives a number it is used; the rest are QS/400's. */

#ifndef QS400_LDAP_H
#define QS400_LDAP_H

#include <lber.h>

/* A session and a chain of messages; their contents are the system's. */
typedef struct ldap LDAP;
typedef struct ldapmsg LDAPMessage;
typedef struct ldapfriendlymap LDAPFriendlyMap;

/* A control: OID at 0, value at 16, criticality at 48. */
typedef struct ldapcontrol {
    char *ldctl_oid;
    struct berval ldctl_value;
    char ldctl_iscritical;
} LDAPControl;

/* A change: operation at 0, type at 16, values at 32. */
typedef struct ldapmod {
    int mod_op;
    char *mod_type;
    union {
        char **modv_strvals;
        struct berval **modv_bvals;
    } mod_vals;
} LDAPMod;
#define mod_values  mod_vals.modv_strvals
#define mod_bvalues mod_vals.modv_bvals

/* A sort key: attribute at 0, ordering rule at 16, reverse at 32. */
typedef struct ldapsortkey {
    char *attributeType;
    char *orderingRule;
    int reverseOrder;
} LDAPSortKey;

/* A parsed URL: 112 bytes. */
typedef struct ldap_url_desc {
    char *lud_host;
    int lud_port;
    char *lud_dn;
    char **lud_attrs;
    int lud_scope;
    char *lud_filter;
    char *lud_string;
} LDAPURLDesc;

/* A virtual list view request: attrvalue set asks by value. */
typedef struct ldapvlvinfo {
    int ldvlv_version;
    unsigned long ldvlv_before_count;
    unsigned long ldvlv_after_count;
    unsigned long ldvlv_offset;
    unsigned long ldvlv_count;
    struct berval *ldvlv_attrvalue;
    struct berval *ldvlv_context;
    void *ldvlv_extradata;
} LDAPVLVInfo;

/* ldap_version: the release as VRM, the protocol, the TLS level. */
typedef struct ldapversion {
    int sdk_version;
    int protocol_version;
    int ssl_version;
} LDAPVersion;

/* The rebind callback (not supported on QS/400). */
typedef int (*LDAPRebindProc)(LDAP *ld, char **dnp, char **passwdp,
    int *authmethodp, int freeit);

/* struct timeval is two four-byte fields, tv_sec and tv_usec. */
struct timeval;

#define LDAP_PORT                              389
#define LDAPS_PORT                             636
#define LDAP_VERSION2                          2
#define LDAP_VERSION3                          3
#define LDAP_VERSION_MIN                       2
#define LDAP_VERSION_MAX                       3
#define LDAP_NO_LIMIT                          0
#define LDAP_OPT_API_INFO                      0
#define LDAP_OPT_DESC                          1
#define LDAP_OPT_DEREF                         2
#define LDAP_OPT_SIZELIMIT                     3
#define LDAP_OPT_TIMELIMIT                     4
#define LDAP_OPT_REFHOPLIMIT                   5
#define LDAP_OPT_REBIND_FN                     6
#define LDAP_OPT_REBIND_ARG                    7
#define LDAP_OPT_REFERRALS                     8
#define LDAP_OPT_RESTART                       9
#define LDAP_OPT_SSL_CIPHER                    10
#define LDAP_OPT_SSL_TIMEOUT                   11
#define LDAP_OPT_DEBUG                         12
#define LDAP_OPT_UTF8_IO                       14
#define LDAP_OPT_PROTOCOL_VERSION              17
#define LDAP_OPT_SERVER_CONTROLS               18
#define LDAP_OPT_CLIENT_CONTROLS               19
#define LDAP_OPT_HOST_NAME                     48
#define LDAP_OPT_ERROR_NUMBER                  49
#define LDAP_OPT_ERROR_STRING                  50
#define LDAP_OPT_MATCHED_DN                    51
#define LDAP_OPT_EXT_ERROR                     52
#define LDAP_OPT_ON                            ((void *) 1)
#define LDAP_OPT_OFF                           ((void *) 0)
#define LDAP_DEREF_NEVER                       0
#define LDAP_DEREF_SEARCHING                   1
#define LDAP_DEREF_FINDING                     2
#define LDAP_DEREF_ALWAYS                      3
#define LDAP_SCOPE_BASE                        0
#define LDAP_SCOPE_ONELEVEL                    1
#define LDAP_SCOPE_SUBTREE                     2
#define LDAP_MOD_ADD                           0
#define LDAP_MOD_DELETE                        1
#define LDAP_MOD_REPLACE                       2
#define LDAP_MOD_INCREMENT                     3
#define LDAP_MOD_BVALUES                       0x80
#define LDAP_AUTH_NONE                         0x00
#define LDAP_AUTH_SIMPLE                       0x80
#define LDAP_AUTH_SASL                         0xA3
#define LDAP_MSG_ONE                           0
#define LDAP_MSG_ALL                           1
#define LDAP_MSG_RECEIVED                      2
#define LDAP_RES_ANY                           (-1)
#define LDAP_RES_BIND                          0x61
#define LDAP_RES_SEARCH_ENTRY                  0x64
#define LDAP_RES_SEARCH_RESULT                 0x65
#define LDAP_RES_MODIFY                        0x67
#define LDAP_RES_ADD                           0x69
#define LDAP_RES_DELETE                        0x6B
#define LDAP_RES_MODRDN                        0x6D
#define LDAP_RES_COMPARE                       0x6F
#define LDAP_RES_SEARCH_REFERENCE              0x73
#define LDAP_RES_EXTENDED                      0x78
#define LDAP_RES_INTERMEDIATE                  0x79
#define LDAP_SUCCESS                           0
#define LDAP_OPERATIONS_ERROR                  1
#define LDAP_PROTOCOL_ERROR                    2
#define LDAP_TIMELIMIT_EXCEEDED                3
#define LDAP_SIZELIMIT_EXCEEDED                4
#define LDAP_COMPARE_FALSE                     5
#define LDAP_COMPARE_TRUE                      6
#define LDAP_STRONG_AUTH_NOT_SUPPORTED         7
#define LDAP_STRONG_AUTH_REQUIRED              8
#define LDAP_PARTIAL_RESULTS                   9
#define LDAP_REFERRAL                          10
#define LDAP_ADMIN_LIMIT_EXCEEDED              11
#define LDAP_UNAVAILABLE_CRITICAL_EXTENSION    12
#define LDAP_CONFIDENTIALITY_REQUIRED          13
#define LDAP_SASL_BIND_IN_PROGRESS             14
#define LDAP_NO_SUCH_ATTRIBUTE                 16
#define LDAP_UNDEFINED_TYPE                    17
#define LDAP_INAPPROPRIATE_MATCHING            18
#define LDAP_CONSTRAINT_VIOLATION              19
#define LDAP_TYPE_OR_VALUE_EXISTS              20
#define LDAP_INVALID_SYNTAX                    21
#define LDAP_NO_SUCH_OBJECT                    32
#define LDAP_ALIAS_PROBLEM                     33
#define LDAP_INVALID_DN_SYNTAX                 34
#define LDAP_IS_LEAF                           35
#define LDAP_ALIAS_DEREF_PROBLEM               36
#define LDAP_INAPPROPRIATE_AUTH                48
#define LDAP_INVALID_CREDENTIALS               49
#define LDAP_INSUFFICIENT_ACCESS               50
#define LDAP_BUSY                              51
#define LDAP_UNAVAILABLE                       52
#define LDAP_UNWILLING_TO_PERFORM              53
#define LDAP_LOOP_DETECT                       54
#define LDAP_NAMING_VIOLATION                  64
#define LDAP_OBJECT_CLASS_VIOLATION            65
#define LDAP_NOT_ALLOWED_ON_NONLEAF            66
#define LDAP_NOT_ALLOWED_ON_RDN                67
#define LDAP_ALREADY_EXISTS                    68
#define LDAP_NO_OBJECT_CLASS_MODS              69
#define LDAP_RESULTS_TOO_LARGE                 70
#define LDAP_AFFECTS_MULTIPLE_DSAS             71
#define LDAP_OTHER                             80
#define LDAP_SERVER_DOWN                       81
#define LDAP_LOCAL_ERROR                       82
#define LDAP_ENCODING_ERROR                    83
#define LDAP_DECODING_ERROR                    84
#define LDAP_TIMEOUT                           85
#define LDAP_AUTH_UNKNOWN                      0x56
#define LDAP_FILTER_ERROR                      87
#define LDAP_USER_CANCELLED                    88
#define LDAP_PARAM_ERROR                       89
#define LDAP_NO_MEMORY                         90
#define LDAP_CONNECT_ERROR                     91
#define LDAP_NOT_SUPPORTED                     92
#define LDAP_CONTROL_NOT_FOUND                 93
#define LDAP_NO_RESULTS_RETURNED               94
#define LDAP_MORE_RESULTS_TO_RETURN            95
#define LDAP_CLIENT_LOOP                       96
#define LDAP_REFERRAL_LIMIT_EXCEEDED           97
#define LDAP_CHANGETYPE_ADD                    1
#define LDAP_CHANGETYPE_DELETE                 2
#define LDAP_CHANGETYPE_MODIFY                 4
#define LDAP_CHANGETYPE_MODDN                  8
#define LDAP_CHANGETYPE_ANY                    15
#define LDAP_PP_WARNING_NONE                   0
#define LDAP_PP_TIME_BEFORE_EXPIRATION         1
#define LDAP_PP_GRACE_LOGINS_REMAINING         2
#define LDAP_PP_NO_ERROR                       (-1)
#define LDAP_PP_PASSWORD_EXPIRED               0
#define LDAP_PP_ACCOUNT_LOCKED                 1
#define LDAP_PP_CHANGE_AFTER_RESET             2
#define LDAP_PP_MOD_NOT_ALLOWED                3
#define LDAP_PP_MUST_SUPPLY_OLD                4
#define LDAP_PP_INSUFFICIENT_QUALITY           5
#define LDAP_PP_TOO_SHORT                      6
#define LDAP_PP_TOO_YOUNG                      7
#define LDAP_PP_IN_HISTORY                     8
#define LDAP_CONTROL_PAGEDRESULTS      "1.2.840.113556.1.4.319"
#define LDAP_CONTROL_SORTREQUEST       "1.2.840.113556.1.4.473"
#define LDAP_CONTROL_SORTRESPONSE      "1.2.840.113556.1.4.474"
#define LDAP_CONTROL_MANAGEDSAIT       "2.16.840.1.113730.3.4.2"
#define LDAP_CONTROL_PERSISTENTSEARCH  "2.16.840.1.113730.3.4.3"
#define LDAP_CONTROL_ENTRYCHANGE       "2.16.840.1.113730.3.4.7"
#define LDAP_CONTROL_PROXYAUTH         "2.16.840.1.113730.3.4.18"
#define LDAP_CONTROL_TREEDELETE        "1.2.840.113556.1.4.805"
#define LDAP_CONTROL_ASSERTION         "1.3.6.1.1.12"
#define LDAP_CONTROL_PWDPOLICY         "1.3.6.1.4.1.42.2.27.8.5.1"
#define LDAP_CONTROL_VLVREQUEST        "2.16.840.1.113730.3.4.9"
#define LDAP_CONTROL_VLVRESPONSE       "2.16.840.1.113730.3.4.10"
#define LDAP_EXOP_START_TLS            "1.3.6.1.4.1.1466.20037"
#define LDAP_EXOP_WHO_AM_I             "1.3.6.1.4.1.4203.1.11.3"
#define LDAP_EXOP_MODIFY_PASSWD        "1.3.6.1.4.1.4203.1.11.1"

LDAP *ldap_init(const char *host, int port);
LDAP *ldap_open(const char *host, int port);
int ldap_unbind(LDAP *ld);
int ldap_unbind_s(LDAP *ld);
int ldap_unbind_ext(LDAP *ld, LDAPControl **sctrls, LDAPControl **cctrls);
int ldap_version(LDAPVersion *ver);
int ldap_set_option(LDAP *ld, int option, const void *invalue);
int ldap_get_option(LDAP *ld, int option, void *outvalue);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
void ldap_set_rebind_proc(LDAP *ld, LDAPRebindProc proc);
int ldap_default_dn_set(LDAP *ld, const char *dn);
int ldap_default_dn_get(LDAP *ld, char **dn);
int ldap_password_set(LDAP *ld, const char *password);
int ldap_password_get(LDAP *ld, char **password);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_proxy_dn_prefix(LDAP *ld, const char *dn);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_proxy_dn_prefix_utf8(LDAP *ld, const char *dn);
int ldap_simple_bind(LDAP *ld, const char *dn, const char *passwd);
int ldap_simple_bind_s(LDAP *ld, const char *dn, const char *passwd);
int ldap_bind(LDAP *ld, const char *dn, const char *cred, int method);
int ldap_bind_s(LDAP *ld, const char *dn, const char *cred, int method);
int ldap_sasl_bind(LDAP *ld, const char *dn, const char *mechanism,
    const struct berval *cred, LDAPControl **sctrls, LDAPControl **cctrls,
    int *msgidp);
int ldap_sasl_bind_s(LDAP *ld, const char *dn, const char *mechanism,
    const struct berval *cred, LDAPControl **sctrls, LDAPControl **cctrls,
    struct berval **servercredp);
int ldap_parse_sasl_bind_result(LDAP *ld, LDAPMessage *res,
    struct berval **servercredp, int freeit);
int ldap_get_bind_controls(LDAP *ld, LDAPControl ***ctrls);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_plugin_sasl_bind_s(LDAP *ld, const char *dn, const char *mechanism,
    const struct berval *cred, LDAPControl **sctrls, LDAPControl **cctrls);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_krb_init_tkt(char *name, char *password);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_krb_cc_destroy(void);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_krb_ctx_destroy(void);
int ldap_search(LDAP *ld, const char *base, int scope, const char *filter,
    char **attrs, int attrsonly);
int ldap_search_s(LDAP *ld, const char *base, int scope, const char *filter,
    char **attrs, int attrsonly, LDAPMessage **res);
int ldap_search_st(LDAP *ld, const char *base, int scope, const char *filter,
    char **attrs, int attrsonly, struct timeval *timeout, LDAPMessage **res);
int ldap_search_ext(LDAP *ld, const char *base, int scope, const char *filter,
    char **attrs, int attrsonly, LDAPControl **sctrls, LDAPControl **cctrls,
    struct timeval *timeout, int sizelimit, int *msgidp);
int ldap_search_ext_s(LDAP *ld, const char *base, int scope, const char *filter,
    char **attrs, int attrsonly, LDAPControl **sctrls, LDAPControl **cctrls,
    struct timeval *timeout, int sizelimit, LDAPMessage **res);
int ldap_url_search(LDAP *ld, const char *url, int attrsonly);
int ldap_url_search_s(LDAP *ld, const char *url, int attrsonly,
    LDAPMessage **res);
int ldap_url_search_st(LDAP *ld, const char *url, int attrsonly,
    struct timeval *timeout, LDAPMessage **res);
int ldap_result(LDAP *ld, int msgid, int all, struct timeval *timeout,
    LDAPMessage **result);
int ldap_msgfree(LDAPMessage *lm);
int ldap_msgtype(LDAPMessage *lm);
int ldap_msgid(LDAPMessage *lm);
int ldap_result2error(LDAP *ld, LDAPMessage *res, int freeit);
int ldap_parse_result(LDAP *ld, LDAPMessage *res, int *errcodep,
    char **matcheddnp, char **errmsgp, char ***referralsp,
    LDAPControl ***sctrlsp, int freeit);
int ldap_parse_extended_result(LDAP *ld, LDAPMessage *res, char **resultoidp,
    struct berval **resultdatap, int freeit);
int ldap_parse_extended_result_w_controls(LDAP *ld, LDAPMessage *res,
    char **resultoidp, struct berval **resultdatap, LDAPControl ***sctrlsp,
    int freeit);
int ldap_parse_reference(LDAP *ld, LDAPMessage *ref, char ***referralsp,
    LDAPControl ***sctrlsp, int freeit);
int ldap_parse_reference_np(LDAP *ld, LDAPMessage *ref, char ***referralsp,
    LDAPControl ***sctrlsp, int freeit);
LDAPMessage *ldap_first_message(LDAP *ld, LDAPMessage *res);
LDAPMessage *ldap_next_message(LDAP *ld, LDAPMessage *msg);
int ldap_count_messages(LDAP *ld, LDAPMessage *res);
LDAPMessage *ldap_first_entry(LDAP *ld, LDAPMessage *res);
LDAPMessage *ldap_next_entry(LDAP *ld, LDAPMessage *entry);
int ldap_count_entries(LDAP *ld, LDAPMessage *res);
LDAPMessage *ldap_first_reference(LDAP *ld, LDAPMessage *res);
LDAPMessage *ldap_next_reference(LDAP *ld, LDAPMessage *ref);
int ldap_count_references(LDAP *ld, LDAPMessage *res);
char *ldap_get_dn(LDAP *ld, LDAPMessage *entry);
char *ldap_first_attribute(LDAP *ld, LDAPMessage *entry, BerElement **ber);
char *ldap_next_attribute(LDAP *ld, LDAPMessage *entry, BerElement *ber);
int ldap_count_attributes(LDAP *ld, LDAPMessage *entry);
char **ldap_get_values(LDAP *ld, LDAPMessage *entry, const char *attr);
struct berval **ldap_get_values_len(LDAP *ld, LDAPMessage *entry,
    const char *attr);
int ldap_count_values(char **vals);
int ldap_count_values_len(struct berval **vals);
void ldap_value_free(char **vals);
void ldap_value_free_len(struct berval **vals);
int ldap_get_entry_controls(LDAP *ld, LDAPMessage *entry,
    LDAPControl ***sctrlsp);
int ldap_get_entry_controls_np(LDAP *ld, LDAPMessage *entry,
    LDAPControl ***sctrlsp);
void ldap_memfree(char *mem);
void ldap_ber_free(BerElement *ber, int freebuf);
void ldap_mods_free(LDAPMod **mods, int freemods);
char **ldap_explode_dn(const char *dn, int notypes);
char **ldap_explode_dn_utf8(const char *dn, int notypes);
char **ldap_explode_rdn(const char *rdn, int notypes);
char **ldap_explode_rdn_utf8(const char *rdn, int notypes);
char **ldap_explode_dns(const char *dns);
char *ldap_dn2ufn(const char *dn);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
char *ldap_friendly_name(char *filename, char *name, LDAPFriendlyMap **map);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
void ldap_free_friendlymap(LDAPFriendlyMap **map);
int ldap_add(LDAP *ld, const char *dn, LDAPMod **attrs);
int ldap_add_s(LDAP *ld, const char *dn, LDAPMod **attrs);
int ldap_add_ext(LDAP *ld, const char *dn, LDAPMod **attrs,
    LDAPControl **sctrls, LDAPControl **cctrls, int *msgidp);
int ldap_add_ext_s(LDAP *ld, const char *dn, LDAPMod **attrs,
    LDAPControl **sctrls, LDAPControl **cctrls);
int ldap_modify(LDAP *ld, const char *dn, LDAPMod **mods);
int ldap_modify_s(LDAP *ld, const char *dn, LDAPMod **mods);
int ldap_modify_ext(LDAP *ld, const char *dn, LDAPMod **mods,
    LDAPControl **sctrls, LDAPControl **cctrls, int *msgidp);
int ldap_modify_ext_s(LDAP *ld, const char *dn, LDAPMod **mods,
    LDAPControl **sctrls, LDAPControl **cctrls);
int ldap_delete(LDAP *ld, const char *dn);
int ldap_delete_s(LDAP *ld, const char *dn);
int ldap_delete_ext(LDAP *ld, const char *dn, LDAPControl **sctrls,
    LDAPControl **cctrls, int *msgidp);
int ldap_delete_ext_s(LDAP *ld, const char *dn, LDAPControl **sctrls,
    LDAPControl **cctrls);
int ldap_modrdn(LDAP *ld, const char *dn, const char *newrdn, int deleteoldrdn);
int ldap_modrdn_s(LDAP *ld, const char *dn, const char *newrdn,
    int deleteoldrdn);
int ldap_rename(LDAP *ld, const char *dn, const char *newrdn,
    const char *newparent, int deleteoldrdn, LDAPControl **sctrls,
    LDAPControl **cctrls, int *msgidp);
int ldap_rename_s(LDAP *ld, const char *dn, const char *newrdn,
    const char *newparent, int deleteoldrdn, LDAPControl **sctrls,
    LDAPControl **cctrls);
int ldap_compare(LDAP *ld, const char *dn, const char *attr, const char *value);
int ldap_compare_s(LDAP *ld, const char *dn, const char *attr,
    const char *value);
int ldap_compare_ext(LDAP *ld, const char *dn, const char *attr,
    const struct berval *bvalue, LDAPControl **sctrls, LDAPControl **cctrls,
    int *msgidp);
int ldap_compare_ext_s(LDAP *ld, const char *dn, const char *attr,
    const struct berval *bvalue, LDAPControl **sctrls, LDAPControl **cctrls);
int ldap_abandon(LDAP *ld, int msgid);
int ldap_abandon_ext(LDAP *ld, int msgid, LDAPControl **sctrls,
    LDAPControl **cctrls);
int ldap_extended_operation(LDAP *ld, const char *reqoid,
    const struct berval *reqdata, LDAPControl **sctrls, LDAPControl **cctrls,
    int *msgidp);
int ldap_extended_operation_s(LDAP *ld, const char *reqoid,
    const struct berval *reqdata, LDAPControl **sctrls, LDAPControl **cctrls,
    char **retoidp, struct berval **retdatap);
int ldap_create_control(const char *oid, BerElement *ber, int iscritical,
    LDAPControl **ctrlp);
int ldap_create_page_control(LDAP *ld, unsigned long pagesize,
    struct berval *cookie, const char iscritical, LDAPControl **output);
int ldap_parse_page_control(LDAP *ld, LDAPControl **ctrls,
    unsigned long *totalcount, struct berval **cookie);
int ldap_create_sort_keylist(LDAPSortKey ***sortkeylist, const char *keystring);
void ldap_free_sort_keylist(LDAPSortKey **sortkeylist);
int ldap_create_sort_control(LDAP *ld, LDAPSortKey **keylist,
    const char iscritical, LDAPControl **output);
int ldap_parse_sort_control(LDAP *ld, LDAPControl **ctrls,
    unsigned long *returncode, char **attribute);
int ldap_create_proxyauth_control(LDAP *ld, char *proxydn, char iscritical,
    LDAPControl **ctrlp);
int ldap_create_persistentsearch_control(LDAP *ld, int changetypes,
    int changesonly, int returnecs, char iscritical, LDAPControl **ctrlp);
int ldap_parse_entrychange_control(LDAP *ld, LDAPControl **ctrls,
    int *changetype, char **prevdn, int *haschangenumber, long *changenumber);
int ldap_create_assertion_control(LDAP *ld, char *filter, char iscritical,
    LDAPControl **ctrlp);
int ldap_create_vlv_control(LDAP *ld, LDAPVLVInfo *vlvinfo,
    LDAPControl **ctrlp);
int ldap_parse_vlv_control(LDAP *ld, LDAPControl **ctrls,
    unsigned long *targetpos, unsigned long *listcount, struct berval **context,
    int *errcode);
int ldap_parse_pwdpolicy_response(LDAPControl **ctrls, int *warning, int *value,
    int *error);
char *ldap_pwdpolicy_err2string(int error);
void ldap_control_free(LDAPControl *ctrl);
void ldap_controls_free(LDAPControl **ctrls);
int ldap_copy_controls(LDAPControl ***to, LDAPControl **from);
int ldap_insert_control(LDAPControl *newcontrol, LDAPControl ***ctrllist);
int ldap_add_control(const char *oid, unsigned long len, char *value,
    int iscritical, LDAPControl ***ctrllist);
int ldap_remove_control(LDAPControl *delcontrol, LDAPControl ***ctrllist,
    int freeit);
int ldap_merge_control_lists(LDAPControl **list1, LDAPControl **list2,
    LDAPControl ***merged);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_create_group_control(LDAP *ld, char **groups, char iscritical,
    LDAPControl **ctrlp);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_create_group_control_from_list(LDAP *ld, char *groups, char iscritical,
    LDAPControl **ctrlp);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_create_modify_groups_only_control(LDAP *ld, char iscritical,
    LDAPControl **ctrlp);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_create_omit_group_referential_integrity_control(LDAP *ld,
    char iscritical, LDAPControl **ctrlp);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_create_limit_num_values_control(LDAP *ld, int maxvalues,
    int maxpertype, char iscritical, LDAPControl **ctrlp);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_parse_limit_num_values_response(LDAP *ld, LDAPControl **ctrls,
    void **response);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
void ldap_free_limit_num_values_response(void *response);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_create_chaining_audit_control(LDAP *ld, char *data, char iscritical,
    LDAPControl **ctrlp);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_create_return_deleted_objects_control(LDAP *ld, char iscritical,
    LDAPControl **ctrlp);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_create_transaction_control(LDAP *ld, char *tranid, char iscritical,
    LDAPControl **ctrlp);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_create_account_status_request(LDAP *ld, char *dn,
    struct berval **request);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_create_effective_pwdpolicy_request(LDAP *ld, char *dn,
    struct berval **request);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_parse_effective_pwdpolicy_response(LDAP *ld, struct berval *response,
    void **policy);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_create_get_file_request(LDAP *ld, char *name, struct berval **request);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_create_group_eval_request(LDAP *ld, char *dn, char **attrs,
    struct berval **request);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_create_locate_entry_request(LDAP *ld, char **dns,
    struct berval **request);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_create_online_backup_request(LDAP *ld, char *path,
    struct berval **request);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_create_resume_role_request(LDAP *ld, char *role,
    struct berval **request);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_create_user_type_request(LDAP *ld, struct berval **request);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_create_password_policy_bind_init_request(LDAP *ld, char *dn,
    struct berval **request);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_create_password_policy_bind_finalize_request(LDAP *ld, char *dn,
    int result, struct berval **request);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_create_abort_transaction_request(LDAP *ld, char *tranid,
    struct berval **request);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_create_commit_transaction_request(LDAP *ld, char *tranid,
    struct berval **request);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_create_prepare_transaction_request(LDAP *ld, char *tranid,
    struct berval **request);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_start_transaction(LDAP *ld, LDAPControl **sctrls, LDAPControl **cctrls,
    int *msgidp);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_start_transaction_s(LDAP *ld, LDAPControl **sctrls,
    LDAPControl **cctrls, char **tranid);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_end_transaction(LDAP *ld, char *tranid, int commit,
    LDAPControl **sctrls, LDAPControl **cctrls, int *msgidp);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_end_transaction_s(LDAP *ld, char *tranid, int commit,
    LDAPControl **sctrls, LDAPControl **cctrls);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_prepare_transaction(LDAP *ld, char *tranid, LDAPControl **sctrls,
    LDAPControl **cctrls, int *msgidp);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_prepare_transaction_s(LDAP *ld, char *tranid, LDAPControl **sctrls,
    LDAPControl **cctrls);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_get_tran_id(LDAP *ld, LDAPMessage *res, char **tranid);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_server_locate(void *request, void **hosts);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
void ldap_server_free_list(void *hosts);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_server_conf_save(char *filename, unsigned long ttl, void *hosts);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_enetwork_domain_set(char *domain);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_enetwork_domain_get(char **domain);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_plugin_pblock_get(void *pb, int arg, void *value);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_plugin_pblock_set(void *pb, int arg, void *value);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
void ldap_plugin_pblock_free(void *pb);
char *ldap_err2string(int err);
void ldap_perror(LDAP *ld, const char *s);
int ldap_get_lderrno(LDAP *ld, char **m, char **s);
int ldap_set_lderrno(LDAP *ld, int e, char *m, char *s);
int ldap_get_errno(LDAP *ld);
int ldap_get_exterror(LDAP *ld);
int ldap_is_ldap_url(const char *url);
int ldap_is_ldap_url_utf8(const char *url);
int ldap_url_parse(const char *url, LDAPURLDesc **ludpp);
int ldap_url_parse_utf8(const char *url, LDAPURLDesc **ludpp);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_url_parse_internal(const char *url, LDAPURLDesc **ludpp);
void ldap_free_urldesc(LDAPURLDesc *ludp);
int ldap_set_iconv_local_codepage(int codepage);
int ldap_set_iconv_local_charset(const char *charset);
int ldap_get_iconv_local_codepage(void);
int ldap_xlate_local_to_utf8(char *inbuf, unsigned long *inlen, char *outbuf,
    unsigned long *outlen);
int ldap_xlate_utf8_to_local(char *inbuf, unsigned long *inlen, char *outbuf,
    unsigned long *outlen);
int ldap_xlate_local_to_unicode(char *inbuf, unsigned long *inlen, char *outbuf,
    unsigned long *outlen);
int ldap_xlate_unicode_to_local(char *inbuf, unsigned long *inlen, char *outbuf,
    unsigned long *outlen);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_xlate_local_to_utf8_wrp(char *inbuf, unsigned long *inlen,
    char *outbuf, unsigned long *outlen);
int ldap_char_size(char *p);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_set_locale(const char *locale);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
char *ldap_get_locale(void);

#include <ldapssl.h>

#endif
