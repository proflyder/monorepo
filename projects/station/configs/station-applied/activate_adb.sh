#!/bin/sh
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
