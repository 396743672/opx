//! 通用有向无环图（DAG）拓扑工具
//!
//! 提供 Kahn 分层排序与三色 DFS 环检测，与具体节点结构解耦。
//! 输入统一抽象为「节点 id → 依赖的其它节点 id 集合」，供服务组（Stack）与
//! 软件级依赖共用。所有函数为纯函数，便于单元测试。

use std::collections::{HashMap, HashSet};

/// Kahn 拓扑分层结果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TopoPlan {
    /// 逐批顺序：layers[0] 为第一批（无依赖），批内成员相互独立。
    pub layers: Vec<Vec<String>>,
    /// 检测到环时返回环路径（位于图上的一圈，顺序为一条可循环路）。
    pub cycle: Option<Vec<String>>,
}

/// 对一张 DAG 做 Kahn 分层排序。
///
/// `nodes`: 参与排序的节点 id 集合。
/// `deps(id)`: 返回某节点的依赖其它节点 id（仅存在于 `nodes` 内的才会被计入入度）。
/// `tiebreak`: 同层内并列节点的稳定排序键（用 `order` 类字段，供稳定输出）。
///
/// 返回分层计划；若存在环则 `layers` 为空、`cycle` 为环路径。
pub fn topo_layers<F>(nodes: &[String], deps: F, tiebreak: &HashMap<String, u32>) -> TopoPlan
where
    F: Fn(&str) -> Vec<String>,
{
    let node_ids: HashSet<String> = nodes.iter().cloned().collect();
    let mut indeg: HashMap<String, usize> = HashMap::new();
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    for n in nodes {
        indeg.insert(n.clone(), 0);
        adj.insert(n.clone(), Vec::new());
    }
    for n in nodes {
        for dep in deps(n) {
            if node_ids.contains(&dep) && dep != *n {
                *indeg.get_mut(&*n).unwrap() += 1;
                adj.get_mut(&dep).unwrap().push(n.clone());
            }
        }
    }

    // 入度为零的节点作为初始队列
    let mut queue: Vec<&String> = nodes.iter().filter(|n| indeg[*n] == 0).collect();
    queue.sort_by_key(|n| tiebreak.get(*n).copied().unwrap_or(0));

    let mut layers: Vec<Vec<String>> = Vec::new();
    let mut visited: usize = 0;

    while !queue.is_empty() {
        let layer_ids: Vec<String> = queue.iter().map(|n| (*n).clone()).collect();
        layers.push(layer_ids);

        let mut next: Vec<&String> = Vec::new();
        for n in &queue {
            if let Some(children) = adj.get(*n) {
                for m in children {
                    let e = indeg.get_mut(m).unwrap();
                    *e -= 1;
                    if *e == 0 {
                        next.push(m);
                    }
                }
            }
        }
        next.sort_by_key(|n| tiebreak.get(*n).copied().unwrap_or(0));
        visited += queue.len();
        queue = next;
    }

    if visited != nodes.len() {
        let cycle = find_cycle(nodes, deps, &node_ids);
        return TopoPlan { layers: Vec::new(), cycle: Some(cycle) };
    }

    TopoPlan { layers, cycle: None }
}

/// 三色 DFS 环检测：返回一条环路径（空表示无环）。
pub fn find_cycle<F>(nodes: &[String], deps: F, node_ids: &HashSet<String>) -> Vec<String>
where
    F: Fn(&str) -> Vec<String>,
{
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    for n in nodes {
        let mut v = Vec::new();
        for dep in deps(n) {
            if node_ids.contains(&dep) {
                v.push(dep);
            }
        }
        adj.insert(n.clone(), v);
    }

    let mut color: HashMap<String, u8> = HashMap::new(); // 0=未访问 1=在栈 2=完成
    let mut trace: Vec<String> = Vec::new();
    let mut result: Vec<String> = Vec::new();

    fn dfs(
        node: &str,
        adj: &HashMap<String, Vec<String>>,
        color: &mut HashMap<String, u8>,
        trace: &mut Vec<String>,
        result: &mut Vec<String>,
    ) -> bool {
        color.insert(node.to_string(), 1);
        trace.push(node.to_string());
        if let Some(children) = adj.get(node) {
            for c in children {
                match color.get(c).copied().unwrap_or(0) {
                    0 => {
                        if dfs(c, adj, color, trace, result) {
                            return true;
                        }
                    }
                    1 => {
                        // 找到环：截取 trace 中从 c 到结尾的一段
                        let pos = trace.iter().position(|x| x == c).unwrap();
                        result.extend(trace[pos..].iter().cloned());
                        result.push(c.clone());
                        return true;
                    }
                    _ => {}
                }
            }
        }
        trace.pop();
        color.insert(node.to_string(), 2);
        false
    }

    for n in nodes {
        if color.get(n).copied().unwrap_or(0) == 0 {
            if dfs(n, &adj, &mut color, &mut trace, &mut result) {
                return result;
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A→B→C 链式：应得 3 层 [C], [B], [A]
    #[test]
    fn test_chain_three_layers() {
        let nodes = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        // a 依赖 b，b 依赖 c
        let deps = |n: &str| -> Vec<String> {
            match n {
                "a" => vec!["b".to_string()],
                "b" => vec!["c".to_string()],
                _ => vec![],
            }
        };
        let plan = topo_layers(&nodes, deps, &HashMap::new());
        assert!(plan.cycle.is_none());
        assert_eq!(plan.layers.len(), 3);
        // 分层 C 最先（无依赖），A 最后
        let flat: Vec<String> = plan.layers.iter().flatten().cloned().collect();
        assert_eq!(flat, vec!["c", "b", "a"]);
    }

    /// 菱形依赖 A←{B,C}←D：B、C 同层，A 依赖两者，D 最先。
    #[test]
    fn test_diamond_dedup_layers() {
        let nodes = vec!["a".to_string(), "b".to_string(), "c".to_string(), "d".to_string()];
        let deps = |n: &str| -> Vec<String> {
            match n {
                "a" => vec!["b".to_string(), "c".to_string()],
                "b" => vec!["d".to_string()],
                "c" => vec!["d".to_string()],
                _ => vec![],
            }
        };
        let plan = topo_layers(&nodes, deps, &HashMap::new());
        assert!(plan.cycle.is_none());
        // 第一层应为 D；B、C 同在一层
        assert_eq!(plan.layers[0], vec!["d".to_string()]);
        let l1: Vec<String> = plan.layers[1].clone();
        assert_eq!(l1.len(), 2);
        assert!(l1.contains(&"b".to_string()) && l1.contains(&"c".to_string()));
        assert_eq!(plan.layers.last().unwrap(), &vec!["a".to_string()]);
    }

    /// 环 A→B→A：检测到环，layers 空。
    #[test]
    fn test_detect_cycle() {
        let nodes = vec!["a".to_string(), "b".to_string()];
        let deps = |n: &str| -> Vec<String> {
            match n {
                "a" => vec!["b".to_string()],
                "b" => vec!["a".to_string()],
                _ => vec![],
            }
        };
        let plan = topo_layers(&nodes, deps, &HashMap::new());
        assert!(plan.layers.is_empty());
        let cycle = plan.cycle.expect("应返回环路径");
        // 环路径应包含 a 与 b 且首尾相接
        assert!(cycle.contains(&"a".to_string()) && cycle.contains(&"b".to_string()));
        assert_eq!(cycle.first(), cycle.last());
    }

    /// 自引用与重复 id 应被忽略，不视为环。
    #[test]
    fn test_self_ref_and_duplicate_ignored() {
        let nodes = vec!["a".to_string(), "b".to_string()];
        let deps = |n: &str| -> Vec<String> {
            match n {
                "a" => vec!["a".to_string(), "b".to_string(), "b".to_string()],
                _ => vec![],
            }
        };
        let plan = topo_layers(&nodes, deps, &HashMap::new());
        assert!(plan.cycle.is_none());
        // a 依赖 b → 两层 [b],[a]
        assert_eq!(plan.layers.len(), 2);
    }

    /// 菱形依赖下的节点：D 入度 2，但访问计数应正确去重，不因重复依赖而误报环。
    #[test]
    fn test_repeated_dep_not_cycle() {
        let nodes = vec!["a".to_string(), "d".to_string()];
        let deps = |n: &str| -> Vec<String> {
            match n {
                "a" => vec!["d".to_string(), "d".to_string()], // 重复依赖 D 两次
                _ => vec![],
            }
        };
        let plan = topo_layers(&nodes, deps, &HashMap::new());
        assert!(plan.cycle.is_none());
        assert_eq!(plan.layers, vec![vec!["d".to_string()], vec!["a".to_string()]]);
    }
}

