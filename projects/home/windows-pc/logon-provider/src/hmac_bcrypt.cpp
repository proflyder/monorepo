// HMAC-SHA256 through CNG (bcrypt.dll), base64 output.
//
// Any failure returns an empty string; callers compare it against the received
// mac and therefore fail closed.

#include <windows.h>
#include <bcrypt.h>

#include <vector>

#include "protocol/protocol.h"

namespace ricardo {
namespace wakegrant {
namespace {

class AlgorithmHandle {
  public:
    ~AlgorithmHandle() {
        if (handle_ != nullptr) BCryptCloseAlgorithmProvider(handle_, 0);
    }
    BCRYPT_ALG_HANDLE* out() { return &handle_; }
    BCRYPT_ALG_HANDLE get() const { return handle_; }

  private:
    BCRYPT_ALG_HANDLE handle_ = nullptr;
};

class HashHandle {
  public:
    ~HashHandle() {
        if (handle_ != nullptr) BCryptDestroyHash(handle_);
    }
    BCRYPT_HASH_HANDLE* out() { return &handle_; }
    BCRYPT_HASH_HANDLE get() const { return handle_; }

  private:
    BCRYPT_HASH_HANDLE handle_ = nullptr;
};

}  // namespace

std::string hmac_sha256_b64(const Bytes& key, std::string_view message) {
    AlgorithmHandle algorithm;
    if (!BCRYPT_SUCCESS(BCryptOpenAlgorithmProvider(
            algorithm.out(), BCRYPT_SHA256_ALGORITHM, nullptr, BCRYPT_ALG_HANDLE_HMAC_FLAG))) {
        return std::string();
    }

    DWORD object_size = 0;
    DWORD produced = 0;
    if (!BCRYPT_SUCCESS(BCryptGetProperty(algorithm.get(), BCRYPT_OBJECT_LENGTH,
                                         reinterpret_cast<PUCHAR>(&object_size),
                                         sizeof(object_size), &produced, 0))) {
        return std::string();
    }

    std::vector<unsigned char> object(object_size);
    HashHandle hash;
    if (!BCRYPT_SUCCESS(BCryptCreateHash(algorithm.get(), hash.out(), object.data(), object_size,
                                         const_cast<PUCHAR>(key.data()),
                                         static_cast<ULONG>(key.size()), 0))) {
        return std::string();
    }

    if (!message.empty() &&
        !BCRYPT_SUCCESS(BCryptHashData(hash.get(), reinterpret_cast<PUCHAR>(
                                                     const_cast<char*>(message.data())),
                                      static_cast<ULONG>(message.size()), 0))) {
        return std::string();
    }

    unsigned char digest[32] = {};
    if (!BCRYPT_SUCCESS(BCryptFinishHash(hash.get(), digest, sizeof(digest), 0))) {
        return std::string();
    }

    Bytes result(digest, digest + sizeof(digest));
    const std::string encoded = b64_encode(result);
    SecureZeroMemory(result.data(), result.size());
    return encoded;
}

}  // namespace wakegrant
}  // namespace ricardo
