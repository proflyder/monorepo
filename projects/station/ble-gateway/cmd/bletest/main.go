package main

import (
	"fmt"
	"os"
	"strings"

	"github.com/godbus/dbus/v5"
)

func main() {
	conn, err := dbus.SystemBus()
	if err != nil {
		panic(err)
	}
	defer conn.Close()

	if len(os.Args) < 3 {
		fmt.Println("usage: BLE_DEST=:1.x bletest <char-index> <value> [status]")
		os.Exit(2)
	}
	dest := os.Getenv("BLE_DEST")
	if dest == "" {
		dest = "com.station.gateway"
	}
	idx := os.Args[1]
	value := os.Args[2]
	path := dbus.ObjectPath("/com/station/gateway/service0/char" + idx)
	obj := conn.Object(dest, path)
	if len(os.Args) > 3 && os.Args[3] == "status" {
		v, err := obj.GetProperty("org.bluez.GattCharacteristic1.Value")
		if err != nil {
			panic(err)
		}
		b, _ := v.Value().([]byte)
		fmt.Printf("status=%s\n", strings.TrimSpace(string(b)))
		return
	}
	if err := obj.Call("org.bluez.GattCharacteristic1.WriteValue", 0, []byte(value), map[string]dbus.Variant{}).Err; err != nil {
		panic(err)
	}
	fmt.Println("write ok")
}
