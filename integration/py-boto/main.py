#!/usr/bin/env python3
import logging

import boto3
boto3.set_stream_logger('', logging.INFO)

session = boto3.session.Session()
s3 = session.client(
    service_name='s3',
    region_name='us-east-1',
    aws_access_key_id='S13i4soN5GKYa6sVtspoRiIQFR_3aK',
    aws_secret_access_key='7Zh-FJc2MbMgKHfXoJiz8pkPQXg-3RXGdUmgFahybRk',
    endpoint_url='http://localhost:8202'
)

s3.create_bucket(
    ACL='private',
    Bucket='foo-bar',
)
