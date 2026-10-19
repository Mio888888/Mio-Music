"""Guard against false-positive parity acceptance using synthetic captures."""
import json
import tempfile
import unittest
from pathlib import Path
from PIL import Image
from compare import compare, ssim

class ComparisonTests(unittest.TestCase):
    def test_same_image_and_large_difference(self):
        white=Image.new('RGB',(32,32),'white')
        self.assertAlmostEqual(ssim(white,white),1.0)
        self.assertLess(ssim(white,Image.new('RGB',(32,32),'black')),0.01)

    def test_layout_and_metadata_cannot_silently_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);ref=root/'ref.json';actual=root/'actual.json';png=root/'image.png'
            Image.new('RGB',(32,32),'white').save(png)
            fixture={'theme':'light','viewport':{'width':32,'height':32,'dpr':1},'elements':{'guide':{'bounds':dict(x=0,y=0,width=32,height=32)}}}
            ref.write_text(json.dumps(fixture)); actual.write_text(json.dumps(fixture))
            self.assertTrue(compare(ref,actual,png,png)['pass'])
            fixture['elements']['guide']['bounds']['x']=2
            actual.write_text(json.dumps(fixture))
            self.assertFalse(compare(ref,actual,png,png)['pass'])
            fixture['elements']={};actual.write_text(json.dumps(fixture))
            self.assertIn('missing visible element: guide',compare(ref,actual,png,png)['errors'])
            fixture['viewport']['width']=31;actual.write_text(json.dumps(fixture))
            self.assertIn('viewport or DPR differs',compare(ref,actual,png,png)['errors'])

if __name__=='__main__': unittest.main()
