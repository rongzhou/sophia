import clsx from "clsx";
import Heading from "@theme/Heading";
import Link from "@docusaurus/Link";
import Layout from "@theme/Layout";

import styles from "./index.module.css";

function HomepageHeader() {
  return (
    <header className={clsx("hero hero--primary", styles.hero)}>
      <div className="container">
        <Heading as="h1" className="hero__title">
          Sophia
        </Heading>
        <p className="hero__subtitle">
          An LLM-native graph programming path beyond code pretraining.
        </p>
        <div className={styles.actions}>
          <Link className="button button--secondary button--lg" to="/docs/language-design">
            Language Design
          </Link>
          <Link className="button button--outline button--secondary button--lg" to="/docs/technical-report-v0-2">
            Technical Report v0.2
          </Link>
        </div>
      </div>
    </header>
  );
}

export default function Home() {
  return (
    <Layout
      title="Sophia"
      description="Sophia is an LLM-native graph programming language and workflow."
    >
      <HomepageHeader />
      <main className={styles.main}>
        <section className="container">
          <div className={styles.grid}>
            <article>
              <h2>Beyond Code Pretraining</h2>
              <p>
                Sophia explores whether programming competence can be shared between
                a semantic model and an external language, checker, and graph workflow.
              </p>
            </article>
            <article>
              <h2>Graph Programming for LLMs</h2>
              <p>
                Programs are organized as ASG nodes and append-only development graphs
                rather than primarily as linear source files for human reading.
              </p>
            </article>
          </div>
        </section>
      </main>
    </Layout>
  );
}
