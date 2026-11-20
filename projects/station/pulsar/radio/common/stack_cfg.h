#pragma once

#define DEFAULT_CHANNEL                 15

#define NV_ENABLE                       1

#define SECURITY_ENABLE                 1


#define	ZCL_CLUSTER_NUM_MAX             16

#define ZCL_REPORTING_TABLE_NUM         4

#define	ZCL_SCENE_TABLE_NUM             8

#define APS_GROUP_TABLE_NUM             8

#define APS_BINDING_TABLE_NUM           8


#if (COORDINATOR)
    #define ZB_COORDINATOR_ROLE         1
#elif (ROUTER)
    #define ZB_ROUTER_ROLE              1
#elif (END_DEVICE)
    #define ZB_ED_ROLE                  1
#endif

