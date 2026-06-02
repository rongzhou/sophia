import Link from "@docusaurus/Link";
import useDocusaurusContext from "@docusaurus/useDocusaurusContext";
import Layout from "@theme/Layout";
import Heading from "@theme/Heading";
import styles from "./index.module.css";

const copy = {
  en: {
    title: "Sophia",
    description: "Deterministic semantic programming for LLM-native systems.",
    subtitle:
      "Browse the English and Chinese documentation for Sophia’s language design, compiler pipeline, workflow graph, WASM codegen, libraries, and testing strategy.",
    docs: "Read Docs",
    concepts: "Start with Concepts",
    features: [
      ["Two-layer system", "Separate nondeterministic LLM exploration from deterministic source, checks, and execution."],
      ["Workflow graph", "Preserve objectives, decisions, artifacts, diagnostics, selection, and materialization as append-only graph state."],
      ["Compiler core", "Keep parsing, semantic analysis, effects, contracts, runtime validation, and codegen deterministic."],
    ],
  },
  "zh-Hans": {
    title: "Sophia",
    description: "面向 LLM-native 系统的确定性语义编程语言。",
    subtitle:
      "浏览 Sophia 的中英文文档：语言设计、编译管线、工作流图、WASM codegen、标准库与测试策略。",
    docs: "阅读文档",
    concepts: "从概念导览开始",
    features: [
      ["两层系统", "把非确定的 LLM 探索与确定性的源码、检查和执行分离。"],
      ["工作流图", "以 append-only 图状态保留目标、决策、产物、诊断、选择与物化过程。"],
      ["编译器核心", "解析、语义分析、effect、contract、运行时验证与 codegen 均保持确定性。"],
    ],
  },
};

export default function Home() {
  const { i18n } = useDocusaurusContext();
  const text = copy[i18n.currentLocale] || copy.en;

  return (
    <Layout title={text.title} description={text.description}>
      <main>
        <section className={styles.hero}>
          <div className={styles.heroInner}>
            <p className={styles.kicker}>LLM-native / Agent-native</p>
            <Heading as="h1" className={styles.title}>
              {text.title}
            </Heading>
            <p className={styles.subtitle}>{text.subtitle}</p>
            <div className={styles.actions}>
              <Link className="button button--primary button--lg" to="/docs/overview">
                {text.docs}
              </Link>
              <Link className="button button--secondary button--lg" to="/docs/concepts">
                {text.concepts}
              </Link>
            </div>
          </div>
        </section>
        <section className={styles.features}>
          {text.features.map(([title, body]) => (
            <article className={styles.feature} key={title}>
              <Heading as="h2">{title}</Heading>
              <p>{body}</p>
            </article>
          ))}
        </section>
      </main>
    </Layout>
  );
}
