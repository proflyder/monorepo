#pragma once

#include "comm_cfg.h"

#define CHIP_TYPE                       TLSR_B91
#define FLASH_CAP_SIZE_1M               1

#define APP_RELEASE                     0x50
#define APP_BUILD                       0x04
#define STACK_RELEASE                   0x40
#define STACK_BUILD                     0x01
#define MANUFACTURER_CODE               0x140A
#define IMAGE_TYPE                      0x0580
#define FILE_VERSION                    ((APP_RELEASE << 24) | (APP_BUILD << 16) | (STACK_RELEASE << 8) | STACK_BUILD)

#define IS_BOOT_LOADER_IMAGE            0
#define RESV_FOR_APP_RAM_CODE_SIZE      0
#define IMAGE_OFFSET                    0x8000

#define NV_BASE_ADDRESS_ZB              0xC0000
#define CFG_FACTORY_RST_CNT             0xD6000
#define CFG_PRE_INSTALL_CODE            0xD7000
