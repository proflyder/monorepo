#pragma once

#define ZB_TEST_ENABLE                          0

#define RF_ENABLE                               1
#define PA_ENABLE                               0

#define GSUART_PRINTF_MODE                      0
#define UART_PRINTF_MODE                        0

#define ZBHCI_UART                              1
#define ZBHCI_USB_CDC                           0
#define ZBHCI_USB_HID                           0
#define ZBHCI_EN                                1
#define MODULE_UART_ENABLE                      1

#define VOLTAGE_DETECT_ENABLE                   0

#define FLASH_PROTECT_ENABLE                    0

#define MODULE_WATCHDOG_ENABLE                  0

#define BDB_NO_AUTO_PERMIT_JOIN                 1

#define CLOCK_SYS_CLOCK_HZ                      48000000
#define UART_IDX                                UART0
#define UART_TX_PIN                             UART0_TX_PB2
#define UART_RX_PIN                             UART0_RX_PB3
#define UART_PIN_CFG()                          drv_uart_pin_set(UART_TX_PIN, UART_RX_PIN)

#include "version_cfg.h"
#include "stack_cfg.h"

#define ZCL_ON_OFF_SUPPORT                      1
#define ZCL_LEVEL_CTRL_SUPPORT                  1
#define ZCL_LIGHT_COLOR_CONTROL_SUPPORT         1
#define ZCL_GROUP_SUPPORT                       1
#define ZCL_SCENE_SUPPORT                       1
#define ZCL_OTA_SUPPORT                         1
#define ZCL_GP_SUPPORT                          0
#define AF_TEST_ENABLE                          0

typedef enum {
    EV_POLL_ED_DETECT,
    EV_POLL_HCI,
    EV_POLL_IDLE,
    EV_POLL_MAX,
} ev_poll_e;
