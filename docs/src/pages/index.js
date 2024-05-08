// @ts-check

import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import FeaturesSection from '@site/src/components/FeaturesSection/index';
import MentionsSection from '@site/src/components/MentionsSection/index';
import RioLogo from '@site/static/assets/rio-logo.svg';
import Heading from '@theme/Heading';
import Layout from '@theme/Layout';
import clsx from 'clsx';

import Translate from '@docusaurus/Translate';
import styles from './index.module.css';

const title = 'Meet Rio';

function HomepageHeader() {
  const { siteConfig } = useDocusaurusContext();

  return (
    <header className={clsx('container', styles.header)}>
      <div className={styles.headerText}>
        <Heading as="h1" className={styles.title}>
          {siteConfig.title}
        </Heading>
        <p className={styles.tagline}>{siteConfig.tagline}</p>
        <div className={styles.actionButtonSection}>
          <Link to="/docs/next/install" className={styles.actionButton}>
            <Translate>Install</Translate>
          </Link>
        </div>
      </div>
      <div className={styles.logoContainer}>
        <div className={styles.logoBackground} />
        <RioLogo className={styles.logo} />
      </div>
    </header>
  );
}

export default function Home() {
  return (
    <Layout
      title={title}
      description="Description will go into a meta tag in <head />"
    >
      <HomepageHeader />
      <main>
        <FeaturesSection />
        <MentionsSection />
      </main>
    </Layout>
  );
}
