import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import HomepageFeatures from '@site/src/components/HomepageFeatures';
import Mention from '@site/src/components/Mention';
import Mentions from '@site/src/data/mentions';
import Heading from '@theme/Heading';
import Layout from '@theme/Layout';
import clsx from 'clsx';

import styles from './index.module.css';

const title = 'Meet Rio';

const Logo = ({ src }) => (
  <div className="logo">
    <img
      src={src}
      onError={() => {
        this.onerror = null;
        this.src = 'assets/rio-logo-512-512.png';
      }}
      alt="Rio Logo"
    />
  </div>
);

function HomepageHeader() {
  const { siteConfig } = useDocusaurusContext();
  return (
    <header className={clsx('hero hero--primary', styles.heroBanner)}>
      <div className="container">
        <Logo src={'assets/logo.svg'} />
        <h1 className="hero__title">
          {title} | {siteConfig.title}
        </h1>
        <p className="hero__subtitle">{siteConfig.tagline}</p>
        <div className={styles.buttons}>
          <Link
            className="button button--secondary button--lg"
            to="/docs/install"
          >
            Install
          </Link>
        </div>
      </div>
    </header>
  );
}

function MentionsSection() {
  let columns = [[], [], []];
  Mentions.filter((mention) => mention.showOnHomepage).forEach((mention, i) =>
    columns[i % 3].push(mention),
  );

  return (
    <div className={clsx(styles.section, styles.sectionAlt)}>
      <div className="container">
        <Heading as="h2" className={clsx('margin-bottom--lg', 'text--center')}>
          Loved by many engineers
        </Heading>
        <div className={clsx('row', styles.mentionsSection)}>
          {columns.map((items, i) => (
            <div className="col col--4" key={i}>
              {items.map((tweet) => (
                <Mention {...tweet} key={tweet.url} />
              ))}
            </div>
          ))}
        </div>
      </div>
    </div>
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
        <HomepageFeatures />
        <MentionsSection />
      </main>
    </Layout>
  );
}
