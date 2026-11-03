// Fixed identifiers of the provider. INSTALL/UNINSTALL SCRIPTS AND TESTS MUST
// KEEP THE SAME GUID: install.ps1, uninstall.ps1, status.ps1 and
// test_install_scripts.py all assert it.

#pragma once

#include <guiddef.h>

namespace ricardo {
namespace komp {

// {9459CCEA-4CCC-4435-89BF-E0E132E0C27F}
inline constexpr GUID kProviderClsid = {0x9459ccea,
                                        0x4ccc,
                                        0x4435,
                                        {0x89, 0xbf, 0xe0, 0xe1, 0x32, 0xe0, 0xc2, 0x7f}};

}  // namespace komp
}  // namespace ricardo
