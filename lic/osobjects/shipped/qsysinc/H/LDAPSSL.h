/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work */
/* is licensed under the terms of the MIT license which can be found in */
/* the root directory of this project.                                  */
/*                                                                      */
/* LDAPSSL: secure connections of QS/400's LDAP client (QGLDCLNT).      */
/* A null key ring is the *SYSTEM certificate store; an application ID  */
/* is a Digital Certificate Manager definition.                         */
/* Generated from runtime/ldapclnt; see docs/DIRSRV.md section 13.      */

#ifndef QS400_LDAPSSL_H
#define QS400_LDAPSSL_H

#include <ldap.h>

#define LDAP_SSL_ALREADY_INITIALIZED           112
#define LDAP_SSL_INITIALIZE_FAILED             113
#define LDAP_SSL_CLIENT_INIT_NOT_CALLED        114
#define LDAP_SSL_PARAM_ERROR                   115
#define LDAP_SSL_HANDSHAKE_FAILED              116
#define LDAP_SSL_GET_CIPHER_FAILED             117
#define LDAP_SSL_NOT_AVAILABLE                 118

LDAP *ldap_ssl_init(char *host, int port, char *name);
int ldap_ssl_client_init(char *keyring, char *keyring_pw, int ssl_timeout,
    int *reason);
int ldap_ssl_environment_init(char *keyring, char *keyring_pw,
    char *keyring_stash, int ssl_timeout, int *reason);
int ldap_ssl_start(LDAP *ld, char *keyring, char *keyring_pw, char *name);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_ssl_pkcs11_client_init(char *module, char *token_label, char *token_pw,
    int ssl_timeout, int *reason);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_ssl_pkcs11_environment_init(char *module, char *token_label,
    char *token_pw, int ssl_timeout, int *reason);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_ssl_set_fips_mode_np(int on);
int ldap_app_ssl_client_init_np(char *app_id, int *reason);
LDAP *ldap_app_ssl_init_np(char *host, int port, char *app_id);
int ldap_app_ssl_start_np(LDAP *ld, char *app_id);
int ldap_start_tls_s_np(LDAP *ld, char *name);
int ldap_start_tls_app_np(LDAP *ld, char *app_id);
int ldap_stop_tls_s_np(LDAP *ld);
/* Answers LDAP_NOT_SUPPORTED on QS/400. */
int ldap_set_cipher(LDAP *ld, char *cipher);

#endif
