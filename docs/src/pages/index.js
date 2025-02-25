// @ts-check

import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import FeaturesSection from '@site/src/components/FeaturesSection/index';
import MentionsSection from '@site/src/components/MentionsSection/index';
import MediaSection from '@site/src/components/MediaSection/index';
import RioLogo from '@site/static/assets/rio-logo.png';
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
          <Link to="/docs/install" className={styles.actionButton}>
            <Translate>Install</Translate>
          </Link>
        </div>
        <div className={styles.actionButtonSection}>
        <iframe
              className={styles.githubStarButton}
              src="https://ghbtns.com/github-btn.html?user=raphamorim&amp;repo=rio&amp;type=star&amp;count=true&amp;size=large"
              width={160}
              height={30}
              title="GitHub Stars"
            />
        </div>
      </div>
      <div className={styles.logoContainer}>
        <div className={styles.logoBackground} />
        <video className={styles.logo} autoPlay={true} loop={false} muted={true} playsInline={true} preLoad={true}>
          <source src={"assets/rio-spinning.mov"} type="video/mp4"/>
          <source src={"assets/rio-spinning.webm"} type="video/webm"/>
        </video>
      </div>
    </header>
  );
}

export default function Home() {
  return (
    <Layout
      title={title}
      description="A modern terminal for the 21st century."
    >
      <HomepageHeader />
      <main>
        <FeaturesSection />
        <MediaSection />
        <MentionsSection />
      </main>
    </Layout>
  );
}
