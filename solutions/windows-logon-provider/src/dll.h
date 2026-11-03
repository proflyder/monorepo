// DLL lifetime helpers shared by the provider and the class factory.

#pragma once

#include <windows.h>

namespace ricardo {
namespace komp {

extern HINSTANCE g_hinst;

void DllAddRef();
void DllRelease();

}  // namespace komp
}  // namespace ricardo
