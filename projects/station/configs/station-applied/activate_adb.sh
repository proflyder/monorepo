#!/bin/sh
# FORTRESS-WRAPPER v2: 'deactivate' -> no-op (maind не может убить adbd),
# 'activate' -> всегда с --network (иначе оригинал снял бы network-маркер).
case " $* " in
  *" deactivate "*)
      echo "`date`: FORTRESS: deactivate blocked" >> /data/quasar/quasar_adb.log
      exit 0
      ;;
esac
case " $* " in
  *" activate "*)
      case " $* " in
        *" --network "*) : ;;
        *) set -- "$@" --network ;;
      esac
      ;;
esac
exec /system/vendor/quasar/activate_adb.sh.orig "$@"
