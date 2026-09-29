pub struct CustomTemplate {
    pub id: &'static str,
    pub name_i18n: &'static str,
    pub executable: &'static str,
    pub args: &'static [&'static str],
    pub config_file_relative: Option<&'static str>,
}

#[cfg(windows)]
const REDIS_SERVER_EXE: &str = "redis-server.exe";
#[cfg(not(windows))]
const REDIS_SERVER_EXE: &str = "redis-server";
#[cfg(windows)]
const NGINX_EXE: &str = "nginx.exe";
#[cfg(not(windows))]
const NGINX_EXE: &str = "nginx";

pub fn builtin_templates() -> &'static [CustomTemplate] {
    &[
        CustomTemplate {
            id: "redis-server",
            name_i18n: "template.redisServer",
            executable: REDIS_SERVER_EXE,
            args: &["{config_file}"],
            config_file_relative: Some("redis.conf"),
        },
        CustomTemplate {
            id: "nginx",
            name_i18n: "template.nginx",
            executable: NGINX_EXE,
            args: &["-g", "daemon off;"],
            config_file_relative: Some("conf/nginx.conf"),
        },
        CustomTemplate {
            id: "generic",
            name_i18n: "template.generic",
            executable: "",
            args: &[],
            config_file_relative: None,
        },
    ]
}
