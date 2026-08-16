// SZ-ORM C++ 绑定头文件
//
// 通过 sz-orm-cpp 的 C ABI（libsz_orm_cpp / sz_orm_cpp.dll）为 C++ 提供
// sz-orm-core 的 Pool/Query API（SQLite 后端，真实可用）。
//
// 使用方式：
//   #include "szorm.h"
//   szorm::Pool pool = szorm::Pool::create("sqlite::memory:", 4);
//   pool.execute("CREATE TABLE ...");
//   std::string json = pool.query("SELECT ...");
//
// 链接：-lsz_orm_cpp（Windows 复制 sz_orm_cpp.dll 到可执行文件目录）

#ifndef SZ_ORM_CPP_SZORM_H
#define SZ_ORM_CPP_SZORM_H

#include <cstdint>
#include <cstdlib>
#include <stdexcept>
#include <string>

#if defined(_WIN32)
#define SZORM_API extern "C" __declspec(dllimport)
#else
#define SZORM_API extern "C"
#endif

// ============ C ABI 声明（对应 Rust 侧 sz_orm_cpp_* 导出） ============

SZORM_API void* sz_orm_cpp_pool_new(const char* dsn, const void* config);
SZORM_API void sz_orm_cpp_pool_free(void* handle);
SZORM_API int sz_orm_cpp_ping(void* handle);
SZORM_API char* sz_orm_cpp_query(void* handle, const char* sql);
SZORM_API void sz_orm_cpp_string_free(char* ptr);

struct SzOrmQueryResultC {
    int32_t success;
    int32_t error_code;
    uint64_t rows_affected;
    uint64_t last_insert_id;
};
SZORM_API SzOrmQueryResultC* sz_orm_cpp_execute(void* handle, const char* sql);
SZORM_API void sz_orm_cpp_result_free(SzOrmQueryResultC* ptr);
SZORM_API uint32_t sz_orm_cpp_version();

namespace szorm {

// ============ C++ RAII 封装 ============

/// SZ-ORM 连接池（SQLite 后端），RAII 管理句柄生命周期
class Pool {
public:
    /// 创建连接池（真实创建）。dsn 示例："sqlite::memory:" / "sqlite://path/to.db"
    static Pool create(const std::string& dsn, uint32_t maxConnections = 4) {
        struct Config {
            uint32_t max_connections;
            uint32_t min_connections;
            uint64_t connect_timeout_ms;
            uint64_t idle_timeout_ms;
        };
        Config cfg{maxConnections, 1, 5000, 300000};
        void* h = sz_orm_cpp_pool_new(dsn.c_str(), &cfg);
        if (h == nullptr) {
            throw std::runtime_error("szorm: pool creation failed for dsn: " + dsn);
        }
        return Pool(h);
    }

    Pool(const Pool&) = delete;
    Pool& operator=(const Pool&) = delete;

    Pool(Pool&& other) noexcept : handle_(other.handle_) { other.handle_ = nullptr; }
    Pool& operator=(Pool&& other) noexcept {
        if (this != &other) {
            close();
            handle_ = other.handle_;
            other.handle_ = nullptr;
        }
        return *this;
    }

    ~Pool() { close(); }

    /// 健康检查（真实 acquire + ping）
    bool ping() const { return handle_ != nullptr && sz_orm_cpp_ping(handle_) == 1; }

    /// 执行查询，返回 JSON 行数组
    std::string query(const std::string& sql) const {
        if (handle_ == nullptr) throw std::runtime_error("szorm: pool is closed");
        char* json = sz_orm_cpp_query(handle_, sql.c_str());
        if (json == nullptr) throw std::runtime_error("szorm: query failed: " + sql);
        std::string result(json);
        sz_orm_cpp_string_free(json);
        return result;
    }

    /// 执行写语句，返回影响行数（<0 表示失败）
    int64_t execute(const std::string& sql) const {
        if (handle_ == nullptr) throw std::runtime_error("szorm: pool is closed");
        SzOrmQueryResultC* res = sz_orm_cpp_execute(handle_, sql.c_str());
        if (res == nullptr) throw std::runtime_error("szorm: execute failed: " + sql);
        int64_t rows = (res->success != 0) ? static_cast<int64_t>(res->rows_affected) : -1;
        sz_orm_cpp_result_free(res);
        return rows;
    }

    /// 释放连接池
    void close() {
        if (handle_ != nullptr) {
            sz_orm_cpp_pool_free(handle_);
            handle_ = nullptr;
        }
    }

    /// 绑定版本号
    static uint32_t version() { return sz_orm_cpp_version(); }

private:
    explicit Pool(void* handle) : handle_(handle) {}
    void* handle_;
};

}  // namespace szorm

#endif  // SZ_ORM_CPP_SZORM_H
