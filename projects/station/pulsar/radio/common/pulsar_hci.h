#pragma once

#include "tl_common.h"
#include "zb_api.h"
#include "bdb.h"

#define HCI_BOOT_IND        0x8907
#define HCI_APP_INFO_REQ    0x0900
#define HCI_APP_INFO_RSP    0x8900

#define ACK_OK              0
#define ACK_BAD_PARAMS      1
#define ACK_UNKNOWN         2
#define ACK_BUSY            3

void hciAck(u16 type, u8 status);
void hciAppInfo(void);
void hciStart(void (*cmd)(u16 type, u16 len, u8 *p));

extern bdb_commissionSetting_t g_commissionSetting;
