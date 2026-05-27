import clsx from "clsx";
import Heading from "@theme/Heading";
import Link from "@docusaurus/Link";
import Layout from "@theme/Layout";
import Translate from "@docusaurus/Translate";

import styles from "./index.module.css";

function HomepageHeader() {
  return (
    <header className={clsx("hero hero--primary", styles.hero)}>
      <div className="container">
        <Heading as="h1" className="hero__title">
          Sophia
        </Heading>
        <p className="hero__subtitle">
          <Translate id="homepage.subtitle" description="Homepage subtitle">
            An LLM-native graph programming path beyond code pretraining.
          </Translate>
        </p>
        <div className={styles.actions}>
          <Link className="button button--secondary button--lg" to="/docs/language-design">
            <Translate id="homepage.languageDesign" description="Homepage language design button">
              Language Design
            </Translate>
          </Link>
          <Link className="button button--outline button--secondary button--lg" to="/docs/heuristic-workflow">
            <Translate id="homepage.heuristicWorkflow" description="Homepage heuristic workflow button">
              Heuristic Workflow
            </Translate>
          </Link>
          <Link className="button button--outline button--secondary button--lg" to="/docs/technical-report-v0-2">
            <Translate id="homepage.technicalReport" description="Homepage technical report button">
              Technical Report v0.2
            </Translate>
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
              <h2>
                <Translate id="homepage.beyondCodePretraining.title" description="Homepage code pretraining feature title">
                  Beyond Code Pretraining
                </Translate>
              </h2>
              <p>
                <Translate
                  id="homepage.beyondCodePretraining.description"
                  description="Homepage code pretraining feature description"
                >
                  Sophia explores whether programming competence can be shared between a semantic model and an
                  external language, checker, and graph workflow.
                </Translate>
              </p>
            </article>
            <article>
              <h2>
                <Translate id="homepage.graphProgramming.title" description="Homepage graph programming feature title">
                  Graph Programming for LLMs
                </Translate>
              </h2>
              <p>
                <Translate
                  id="homepage.graphProgramming.description"
                  description="Homepage graph programming feature description"
                >
                  Programs are organized as ASG nodes and append-only development graphs rather than primarily as
                  linear source files for human reading.
                </Translate>
              </p>
            </article>
          </div>
        </section>
      </main>
    </Layout>
  );
}
