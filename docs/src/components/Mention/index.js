/**
 * Copyright (c) Facebook, Inc. and its affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import React from 'react';

import clsx from 'clsx';

import Link from '@docusaurus/Link';
import styles from './styles.module.css';

export default function Tweet({
  url,
  handle,
  name,
  content,
  date,
  githubUsername,
}) {
  return (
    <div className={clsx('card', styles.mention)}>
      <div className="card__header">
        <div className="avatar">
          <img
            alt={name}
            className="avatar__photo"
            src={`https://unavatar.io/twitter/${handle}?fallback=https://github.com/${githubUsername}.png`}
            width="48"
            height="48"
            loading="lazy"
          />
          <div className={clsx('avatar__intro', styles.mentionMeta)}>
            <strong className="avatar__name">{name}</strong>
            <span>@{handle}</span>
          </div>
        </div>
      </div>

      <div className={clsx('card__body', styles.mention)}>{content}</div>

      <div className="card__footer">
        <Link className={clsx(styles.mentionMeta, styles.mentionDate)} to={url}>
          {date}
        </Link>
      </div>
    </div>
  );
}
