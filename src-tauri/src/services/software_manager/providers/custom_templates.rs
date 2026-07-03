use crate::models::software::CustomHealthSpec;

pub struct CustomTemplate {
    pub id: &'static str,
    pub name_i18n: &'static str,
    pub executable: &'static str,
    pub args: &'static [&'static str],
    pub config_file_relative: Option<&'static str>,
}

impl CustomTemplate {
    /// 返回该模板的默认健康检查 spec
    pub fn default_health_spec(&self) -> CustomHealthSpec {
        match self.id {
            "redis-server" => CustomHealthSpec::Tcp { port: 6379 },
            "nginx" => CustomHealthSpec::Http {
                url: "http://127.0.0.1/".to_string(),
                expected_status: 200,
            },
            _ => CustomHealthSpec::None,
        }
    }
}

pub fn builtin_templates() -> &'static [CustomTemplate] {
    &[
        CustomTemplate {
            id: "redis-server",
            name_i18n: "template.redisServer",
            executable: "redis-server.exe",
            args: &["{config_file}"],
            config_file_relative: Some("redis.conf"),
        },
        CustomTemplate {
            id: "nginx",
            name_i18n: "template.nginx",
            executable: "nginx.exe",
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

pub fn find_template(id: &str) -> Option<&'static CustomTemplate> {
    builtin_templates().iter().find(|t| t.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_templates_has_three_entries() {
        assert_eq!(builtin_templates().len(), 3);
    }

    #[test]
    fn redis_template_uses_tcp_6379() {
        let t = find_template("redis-server").unwrap();
        assert_eq!(t.executable, "redis-server.exe");
        assert_eq!(t.config_file_relative, Some("redis.conf"));
        assert!(matches!(
            t.default_health_spec(),
            CustomHealthSpec::Tcp { port: 6379 }
        ));
    }

    #[test]
    fn nginx_template_uses_http_200() {
        let t = find_template("nginx").unwrap();
        assert_eq!(t.executable, "nginx.exe");
        assert_eq!(t.config_file_relative, Some("conf/nginx.conf"));
        assert!(matches!(
            t.default_health_spec(),
            CustomHealthSpec::Http {
                expected_status: 200,
                ..
            }
        ));
    }

    #[test]
    fn generic_template_has_no_health_check() {
        let t = find_template("generic").unwrap();
        assert!(t.executable.is_empty());
        assert!(matches!(t.default_health_spec(), CustomHealthSpec::None));
        assert!(t.config_file_relative.is_none());
    }

    #[test]
    fn find_template_returns_none_for_unknown() {
        assert!(find_template("nonexistent").is_none());
    }

    #[test]
    fn all_templates_have_unique_ids() {
        let ids: Vec<_> = builtin_templates().iter().map(|t| t.id).collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique.len(), "模板 id 应唯一");
    }

    #[test]
    fn all_templates_have_non_empty_name_i18n() {
        for t in builtin_templates() {
            assert!(!t.name_i18n.is_empty(), "模板 {} 的 name_i18n 不应为空", t.id);
        }
    }
}
