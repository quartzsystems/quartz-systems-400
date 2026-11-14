/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work */
/* is licensed under the terms of the MIT license which can be found in */
/* the root directory of this project.                                  */
/*                                                                      */
/* LBER: the BER encoder and decoder of QS/400's LDAP client (QGLDCLNT). */
/* Generated from runtime/ldapclnt; see docs/DIRSRV.md section 13.      */

#ifndef QS400_LBER_H
#define QS400_LBER_H

/* Tags and lengths are four bytes. */
typedef unsigned long ber_tag_t;
typedef unsigned long ber_len_t;

/* An element being encoded or decoded; its contents are the system's. */
typedef struct berelement BerElement;

/* A counted value: bv_len at offset 0, bv_val at 16. */
struct berval {
    ber_len_t bv_len;
    char *bv_val;
};

#define LBER_ERROR   ((ber_tag_t) -1)
#define LBER_DEFAULT ((ber_tag_t) -1)
#define LBER_BOOLEAN                           0x01
#define LBER_INTEGER                           0x02
#define LBER_BITSTRING                         0x03
#define LBER_OCTETSTRING                       0x04
#define LBER_NULL                              0x05
#define LBER_ENUMERATED                        0x0A
#define LBER_SEQUENCE                          0x30
#define LBER_SET                               0x31
#define LBER_USE_DER                           0x01

/* ber_printf and ber_scanf format letters: see docs/DIRSRV.md section 13. */
BerElement *ber_alloc_t(int options);
BerElement *ber_init(struct berval *bv);
void ber_init2(BerElement *ber, struct berval *bv, int options);
int ber_flatten(BerElement *ber, struct berval **bvptr);
void ber_free(BerElement *ber, int freebuf);
int ber_printf(BerElement *ber, const char *fmt, ...);
ber_tag_t ber_scanf(BerElement *ber, const char *fmt, ...);
ber_tag_t ber_peek_tag(BerElement *ber, ber_len_t *len);
ber_tag_t ber_skip_tag(BerElement *ber, ber_len_t *len);
ber_tag_t ber_get_tag(BerElement *ber);
ber_tag_t ber_skip_element(BerElement *ber, struct berval *bv);
ber_tag_t ber_first_element(BerElement *ber, ber_len_t *len, char **last);
ber_tag_t ber_next_element(BerElement *ber, ber_len_t *len, char *last);
struct berval *ber_bvdup(struct berval *bv);
void ber_bvfree(struct berval *bv);
void ber_bvecfree(struct berval **bv);

#endif
