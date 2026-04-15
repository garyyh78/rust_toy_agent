#!/usr/bin/env python3
import random
import os

def create_homer_text():
    """Create text in Homer's epic style with epithets and invocations"""
    text = """
    Sing, O Muse, of the wrath of Achilles, son of Peleus, that brought countless woes upon the Achaeans. 
    Many a brave soul did it send hurrying down to Hades, and many a hero did it yield a prey to dogs and vultures. 
    Swift-footed Achilles, lion-hearted warrior, raged with fury untamed. 
    The wine-dark sea roared against the shores, and the bronze-clad warriors clashed with mighty force.
    Divine Apollo, god of the silver bow, rained arrows upon the Greek camp. 
    Hector of the shining helm, breaker of horses, led the Trojan charge with courage unyielding.
    Grey-eyed Athena, goddess of wisdom, watched from Olympus, her heart divided.
    The rosy-fingered dawn appeared each day, bringing light to the bloody field of battle.
    Nestor, the wise old horseman, spoke with honeyed words, counseling patience and strategy.
    The wine-dark sea, the swift-footed Achilles, the grey-eyed Athena - these epithets echo through time.
    Bronze weapons gleamed in the sunlight, and the earth trembled beneath the charge of chariots.
    The gods themselves took sides, some favoring the Greeks, others the Trojans in their high-walled city.
    Fate spun her thread, and mortal men could not escape what was woven for them.
    The ships with black prows waited on the shore, ready to carry the warriors home or to their doom.
    """
    # Add more lines to reach ~200 words
    more_text = """
    Great Agamemnon, king of men, wielded his scepter with authority born of lineage divine.
    The Myrmidons, fierce followers of Achilles, stood ready at their master's command.
    Patroclus, beloved companion, donned the armor of Achilles, bringing hope to the Greeks.
    The river Scamander flowed red with blood, choked with bodies of fallen heroes.
    Priam, aged king of Troy, wept for his sons lost in the relentless conflict.
    The walls of Troy, built by Poseidon and Apollo, stood strong against the assault.
    Helen, face that launched a thousand ships, watched from the ramparts with sorrow.
    The funeral pyres burned day and night, sending smoke to the heavens as offering.
    """
    return (text + more_text).strip()

def create_shakespeare_text():
    """Create text in Shakespeare's iambic style with thee/thou"""
    text = """
    To be, or not to be, that is the question:
    Whether 'tis nobler in the mind to suffer
    The slings and arrows of outrageous fortune,
    Or to take arms against a sea of troubles,
    And by opposing end them. To die, to sleep;
    No more; and by a sleep to say we end
    The heart-ache and the thousand natural shocks
    That flesh is heir to: 'tis a consummation
    Devoutly to be wish'd. To die, to sleep;
    To sleep, perchance to dream; ay, there's the rub;
    For in that sleep of death what dreams may come
    When we have shuffled off this mortal coil,
    Must give us pause. There's the respect
    That makes calamity of so long life.
    """
    # Add more lines to reach ~200 words
    more_text = """
    But soft! What light through yonder window breaks?
    It is the east, and Juliet is the sun.
    Arise, fair sun, and kill the envious moon,
    Who is already sick and pale with grief,
    That thou her maid art far more fair than she.
    Be not her maid, since she is envious;
    Her vestal livery is but sick and green
    And none but fools do wear it; cast it off.
    It is my lady, O, it is my love!
    O, that she knew she were!
    She speaks yet she says nothing: what of that?
    Her eye discourses; I will answer it.
    I am too bold, 'tis not to me she speaks.
    Two of the fairest stars in all the heaven,
    Having some business, do entreat her eyes
    To twinkle in their spheres till they return.
    """
    return (text + more_text).strip()

def create_whitman_text():
    """Create text in Whitman's free verse style with cataloging and nature"""
    text = """
    I celebrate myself, and sing myself,
    And what I assume you shall assume,
    For every atom belonging to me as good belongs to you.
    
    I loaf and invite my soul,
    I lean and loaf at my ease observing a spear of summer grass.
    
    My tongue, every atom of my blood, form'd from this soil, this air,
    Born here of parents born here from parents the same, and their parents the same,
    I, now thirty-seven years old in perfect health begin,
    Hoping to cease not till death.
    
    Creeds and schools in abeyance,
    Retiring back a while sufficed at what they are, but never forgotten,
    I harbor for good or bad, I permit to speak at every hazard,
    Nature without check with original energy.
    """
    # Add more lines to reach ~200 words
    more_text = """
    The carpenter dresses his plank, the tongue of his foreplane whistles its wild ascending lisp,
    The married and unmarried children ride home to their Thanksgiving dinner,
    The pilot seizes the king-pin, he heaves down with a strong arm,
    The mate stands braced in the whale-boat, lance and harpoon are ready,
    The duck-shooter walks by silent and cautious stretches,
    The deacons are ordain'd with cross'd hands at the altar,
    The spinning-girl retreats and advances to the hum of the big wheel,
    The farmer stops by the bars as he walks on a First-day loafe and looks at the oats and rye,
    The lunatic is carried at last to the asylum a confirm'd case,
    (He will never sleep any more as he did in the cot in his mother's bed-room;)
    The jour printer with gray head and gaunt jaws works at his case,
    He turns his quid of tobacco while his eyes blurr with the manuscript;
    The malform'd limbs are tied to the surgeon's table,
    What is removed drops horribly in a pail;
    The quadroon girl is sold at the auction-stand, the drunkard nods by the bar-room stove,
    The machinist rolls up his sleeves, the policeman travels his beat, the gate-keeper marks who pass,
    The young fellow drives the express-wagon, (I love him, though I do not know him;)
    The half-breed straps on his light boots to compete in the race,
    The western turkey-shooting draws old and young, some lean on their rifles, some sit on logs,
    Out from the crowd steps the marksman, takes his position, levels his piece;
    The groups of newly-come immigrants cover the wharf or levee,
    As the woolly-pates hoe in the sugar-field, the overseer views them from his saddle,
    The bugle calls in the ball-room, the gentlemen run for their partners, the dancers bow to each other,
    The youth lies awake in the cedar-roof'd garret and harks to the musical rain,
    The Wolverine sets traps on the creek that helps fill the Huron,
    The squaw wrapt in her yellow-hemm'd cloth is offering moccasins and bead-bags for sale,
    The connoisseur peers along the exhibition-gallery with half-shut eyes bent sideways,
    As the deck-hands make fast the steamboat, the plank is thrown for the shore-going passengers,
    The young sister holds out the skein while the elder sister winds it off in a ball, and stops now and then for the knots,
    The one-year wife is recovering and happy having a week ago borne her first child,
    The clean-hair'd Yankee girl works with her sewing-machine or in the factory or mill,
    The paving-man leans on his two-handed rammer, the reporter's lead flies swiftly over the note-book, the sign-painter is lettering with blue and gold,
    The canal boy trots on the tow-path, the book-keeper counts at his desk, the shoemaker waxes his thread,
    The conductor beats time for the band and all the performers follow him,
    The child is baptized, the convert is making his first professions,
    The regatta is spread on the bay, the race is begun, (how the white sails sparkle!)
    The drover watching his drove sings out to them that would stray,
    The pedler sweats with his pack on his back, (the purchaser higgling about the odd cent;)
    The bride unrumples her white dress, the minute-hand of the clock moves slowly,
    The opium-eater reclines with rigid head and just-open'd lips,
    The prostitute draggles her shawl, her bonnet bobs on her tipsy and pimpled neck,
    The crowd laugh at her blackguard oaths, the men jeer and wink to each other,
    (Miserable! I do not laugh at your oaths nor jeer you;)
    The President holding a cabinet council is surrounded by the great Secretaries,
    On the piazza walk three matrons stately and friendly with twined arms,
    The crew of the fish-smack pack repeated layers of halibut in the hold,
    The Missourian crosses the plains toting his wares and his cattle,
    As the fare-collector goes through the train he gives notice by the jingling of loose change,
    The floor-men are laying the floor, the tinners are tinning the roof, the masons are calling for mortar,
    In single file each shouldering his hod pass onward the laborers;
    Seasons pursuing each other the indescribable crowd is gather'd, it is the fourth of Seventh-month, (what salutes of cannon and small arms!)
    Seasons pursuing each other the plougher ploughs, the mower mows, and the winter-grain falls in the ground;
    Off on the lakes the pike-fisher watches and waits by the hole in the frozen surface,
    The stumps stand thick round the clearing, the squatter strikes deep with his axe,
    Flatboatmen make fast towards dusk near the cotton-wood or pecan-trees,
    Coon-seekers go through the regions of the Red river or through those drain'd by the Tennessee, or through those of the Arkansas,
    Torches shine in the dark that hangs on the Chattahooche or Altamahaw,
    Patriarchs sit at supper with sons and grandsons and great-grandsons around them,
    In walls of adobie, in canvas tents, rest hunters and trappers after their day's sport,
    The city sleeps and the country sleeps,
    The living sleep for their time, the dead sleep for their time,
    The old husband sleeps by his wife and the young husband sleeps by his wife;
    And these tend inward to me, and I tend outward to them,
    And such as it is to be of these more or less I am,
    And of these one and all I weave the song of myself.
    """
    return (text + more_text).strip()

def create_milton_text():
    """Create text in Milton's blank verse style with Latinate vocabulary and theological themes"""
    text = """
    Of Man's first disobedience, and the fruit
    Of that forbidden tree whose mortal taste
    Brought death into the World, and all our woe,
    With loss of Eden, till one greater Man
    Restore us, and regain the blissful Seat,
    Sing, Heavenly Muse, that, on the secret top
    Of Oreb, or of Sinai, didst inspire
    That Shepherd who first taught the chosen seed
    In the beginning how the heavens and earth
    Rose out of Chaos: or, if Sion hill
    Delight thee more, and Siloa's brook that flowed
    Fast by the oracle of God, I thence
    Invoke thy aid to my adventurous song,
    That with no middle flight intends to soar
    Above the Aonian mount, while it pursues
    Things unattempted yet in prose or rhyme.
    """
    # Add more lines to reach ~200 words
    more_text = """
    And chiefly Thou, O Spirit, that dost prefer
    Before all temples the upright heart and pure,
    Instruct me, for Thou know'st; Thou from the first
    Wast present, and, with mighty wings outspread,
    Dove-like sat'st brooding on the vast Abyss,
    And mad'st it pregnant: what in me is dark
    Illumine, what is low raise and support;
    That, to the height of this great argument,
    I may assert Eternal Providence,
    And justify the ways of God to men.
    
    Say first—for Heaven hides nothing from thy view,
    Nor the deep tract of Hell—say first what cause
    Moved our grand parents, in that happy state,
    Favoured of Heaven so highly, to fall off
    From their Creator, and transgress his will
    For one restraint, lords of the World besides.
    Who first seduced them to that foul revolt?
    The infernal Serpent; he it was whose guile,
    Stirred up with envy and revenge, deceived
    The mother of mankind, what time his pride
    Had cast him out from Heaven, with all his host
    Of rebel Angels, by whose aid, aspiring
    To set himself in glory above his peers,
    He trusted to have equalled the Most High,
    If he opposed, and with ambitious aim
    Against the throne and monarchy of God,
    Raised impious war in Heaven and battle proud,
    With vain attempt. Him the Almighty Power
    Hurled headlong flaming from the ethereal sky,
    With hideous ruin and combustion, down
    To bottomless perdition, there to dwell
    In adamantine chains and penal fire,
    Who durst defy the Omnipotent to arms.
    """
    return (text + more_text).strip()

def main():
    # Define authors and their creation functions
    authors = {
        "Homer": create_homer_text,
        "Shakespeare": create_shakespeare_text,
        "Whitman": create_whitman_text,
        "Milton": create_milton_text
    }
    
    # Create list of files
    files = ["file1.txt", "file2.txt", "file3.txt", "file4.txt"]
    
    # Shuffle the assignment
    author_list = list(authors.keys())
    random.shuffle(author_list)
    
    # Create mapping
    mapping = {}
    for i, filename in enumerate(files):
        author = author_list[i]
        mapping[filename] = author
    
    # Write files
    for filename, author in mapping.items():
        text = authors[author]()
        with open(filename, 'w') as f:
            f.write(text)
        print(f"Created {filename} in style of {author}")
    
    # Write answers.txt
    with open("answers.txt", 'w') as f:
        for filename, author in mapping.items():
            f.write(f"{filename} -> {author}\n")
    
    print("\nMapping written to answers.txt:")
    for filename, author in mapping.items():
        print(f"{filename} -> {author}")

if __name__ == "__main__":
    main()